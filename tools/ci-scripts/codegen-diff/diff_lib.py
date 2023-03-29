#  Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
#  SPDX-License-Identifier: Apache-2.0

import os
import sys
import subprocess
import shlex

HEAD_BRANCH_NAME = "__tmp-localonly-head"
BASE_BRANCH_NAME = "__tmp-localonly-base"
OUTPUT_PATH = "tmp-codegen-diff"

COMMIT_AUTHOR_NAME = "GitHub Action (generated code preview)"
COMMIT_AUTHOR_EMAIL = "generated-code-action@github.com"

CDN_URL = "https://d2luzm2xt3nokh.cloudfront.net"


def generate_and_commit_generated_code(revision_sha, targets=None):
    targets = targets or ['codegen-client-test', 'codegen-server-test', 'aws:sdk']
    # Clean the build artifacts before continuing
    run("rm -rf aws/sdk/build")
    if 'codegen-server-test' in targets:
        run("cd rust-runtime/aws-smithy-http-server-python/examples && make distclean", shell=True)
    run("./gradlew codegen-core:clean codegen-client:clean codegen-server:clean aws:sdk-codegen:clean")

    # Generate code
    tasks = ' '.join([f'{t}:assemble' for t in targets])
    run(f"./gradlew --rerun-tasks {tasks}")
    if 'codegen-server-test' in targets:
        run("cd rust-runtime/aws-smithy-http-server-python/examples && make build", shell=True, check=False)

    # Move generated code into codegen-diff/ directory
    run(f"rm -rf {OUTPUT_PATH}")
    run(f"mkdir {OUTPUT_PATH}")
    if 'aws:sdk' in targets:
        run(f"mv aws/sdk/build/aws-sdk {OUTPUT_PATH}/")
    for target in ['codegen-client', 'codegen-server']:
        if target in targets:
            run(f"mv {target}/build/smithyprojections/{target} {OUTPUT_PATH}/")
            if target == 'codegen-server-test':
                run(f"mv rust-runtime/aws-smithy-http-server-python/examples/pokemon-service-server-sdk/ {OUTPUT_PATH}/codegen-server-test-python/", check=False)

    # Clean up the SDK directory
    run(f"rm -f {OUTPUT_PATH}/aws-sdk/versions.toml")

    # Clean up the client-test folder
    run(f"rm -rf {OUTPUT_PATH}/codegen-client-test/source")
    run(f"find {OUTPUT_PATH}/codegen-client-test | "
        f"grep -E 'smithy-build-info.json|sources/manifest|model.json' | "
        f"xargs rm -f", shell=True)

    # Clean up the server-test folder
    run(f"rm -rf {OUTPUT_PATH}/codegen-server-test/source")
    run(f"find {OUTPUT_PATH}/codegen-server-test | "
        f"grep -E 'smithy-build-info.json|sources/manifest|model.json' | "
        f"xargs rm -f", shell=True)

    run(f"git add -f {OUTPUT_PATH}")
    run(f"git -c 'user.name=GitHub Action (generated code preview)' "
        f"-c 'user.name={COMMIT_AUTHOR_NAME}' "
        f"-c 'user.email={COMMIT_AUTHOR_EMAIL}' "
        f"commit --no-verify -m 'Generated code for {revision_sha}' --allow-empty")


def make_diff(title, path_to_diff, base_commit_sha, head_commit_sha, suffix, whitespace):
    whitespace_flag = "" if whitespace else "-b"
    diff_exists = get_cmd_status(f"git diff --quiet {whitespace_flag} "
                                 f"{BASE_BRANCH_NAME} {HEAD_BRANCH_NAME} -- {path_to_diff}")
    if diff_exists == 0:
        eprint(f"No diff output for {base_commit_sha}..{head_commit_sha} ({suffix})")
        return None
    else:
        partial_output_path = f"{base_commit_sha}/{head_commit_sha}/{suffix}"
        full_output_path = f"{OUTPUT_PATH}/{partial_output_path}"
        run(f"mkdir -p {full_output_path}")
        run(f"git diff --output=codegen-diff.txt -U30 {whitespace_flag} {BASE_BRANCH_NAME} {HEAD_BRANCH_NAME} -- {path_to_diff}")

        # Generate HTML diff. This uses the `difftags` tool from the `tools/` directory.
        # All arguments after the first `--` go to the `git diff` command.
        whitespace_context = "" if whitespace else "(ignoring whitespace)"
        subtitle = f"rev. {head_commit_sha} {whitespace_context}"
        diff_cmd = f"difftags --output-dir {full_output_path} --title \"{title}\" --subtitle \"{subtitle}\" codegen-diff.txt"
        eprint(f"Running diff cmd: {diff_cmd}")
        run(diff_cmd)
        return f"{partial_output_path}/index.html"


def diff_link(diff_text, empty_diff_text, diff_location, alternate_text, alternate_location):
    if diff_location is None:
        return empty_diff_text
    else:
        return f"[{diff_text}]({CDN_URL}/codegen-diff/{diff_location}) ([{alternate_text}]({CDN_URL}/codegen-diff/{alternate_location}))"


def make_diffs(base_commit_sha, head_commit_sha):
    sdk_ws = make_diff("AWS SDK", f"{OUTPUT_PATH}/aws-sdk", base_commit_sha,
                       head_commit_sha, "aws-sdk", whitespace=True)
    sdk_nows = make_diff("AWS SDK", f"{OUTPUT_PATH}/aws-sdk", base_commit_sha, head_commit_sha,
                         "aws-sdk-ignore-whitespace", whitespace=False)
    client_ws = make_diff("Client Test", f"{OUTPUT_PATH}/codegen-client-test", base_commit_sha,
                          head_commit_sha, "client-test", whitespace=True)
    client_nows = make_diff("Client Test", f"{OUTPUT_PATH}/codegen-client-test", base_commit_sha,
                            head_commit_sha, "client-test-ignore-whitespace", whitespace=False)
    server_ws = make_diff("Server Test", f"{OUTPUT_PATH}/codegen-server-test", base_commit_sha,
                          head_commit_sha, "server-test", whitespace=True)
    server_nows = make_diff("Server Test", f"{OUTPUT_PATH}/codegen-server-test", base_commit_sha,
                            head_commit_sha, "server-test-ignore-whitespace", whitespace=False)
    server_ws_python = make_diff("Server Test Python", f"{OUTPUT_PATH}/codegen-server-test-python", base_commit_sha,
                                 head_commit_sha, "server-test-python", whitespace=True)
    server_nows_python = make_diff("Server Test Python", f"{OUTPUT_PATH}/codegen-server-test-python", base_commit_sha,
                                   head_commit_sha, "server-test-python-ignore-whitespace", whitespace=False)

    sdk_links = diff_link('AWS SDK', 'No codegen difference in the AWS SDK',
                          sdk_ws, 'ignoring whitespace', sdk_nows)
    client_links = diff_link('Client Test', 'No codegen difference in the Client Test',
                             client_ws, 'ignoring whitespace', client_nows)
    server_links = diff_link('Server Test', 'No codegen difference in the Server Test',
                             server_ws, 'ignoring whitespace', server_nows)
    server_links_python = diff_link('Server Test Python', 'No codegen difference in the Server Test Python',
                                    server_ws_python, 'ignoring whitespace', server_nows_python)
    # Save escaped newlines so that the GitHub Action script gets the whole message
    return "A new generated diff is ready to view.\\n" \
           f"- {sdk_links}\\n" \
           f"- {client_links}\\n" \
           f"- {server_links}\\n" \
           f"- {server_links_python}\\n"


def write_to_file(path, text):
    with open(path, "w") as file:
        file.write(text)


# Prints to stderr
def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)


# Runs a shell command
def run(command, shell=False, check=True):
    if not shell:
        command = shlex.split(command)
    subprocess.run(command, stdout=sys.stderr, stderr=sys.stderr, shell=shell, check=check)


# Returns the output from a shell command. Bails if the command failed
def get_cmd_output(command):
    result = subprocess.run(shlex.split(command), capture_output=True, check=True)
    return result.stdout.decode("utf-8").strip()


# Runs a shell command and returns its exit status
def get_cmd_status(command):
    return subprocess.run(command, capture_output=True, shell=True).returncode
