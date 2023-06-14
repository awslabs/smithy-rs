#
# Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
#

import itertools


def main():
    markdown = parser(read_file())
    print(markdown)
    # write file
    with open("/tmp/compiletime-benchmark.md", "w") as f:
        f.write(markdown)
        f.flush()
        f.close()


def read_file() -> itertools.chain[str]:
    # read file
    f = open("/tmp/compiletime-benchmark.txt", "r").read()
    iter = map(lambda x: x.split("END"), f.split("START"))
    iter = itertools.chain.from_iterable(iter)
    return iter


def parser(iter: itertools.chain[str]) -> str:
    # I could've used a dataframe like pandas but this works.
    markdown = """
    | sdk name | dev | release | dev all features | release all features |
    | -------- | --- | ------- | ---------------- | -------------------- |
    """

    for i in iter:
        outputs = []
        print(i)
        for l in i.splitlines():
            if not "+" in l:
                outputs.append(l.replace(" seconds", ""))

        if len(outputs) != 6:
            continue
        outputs = outputs[1:]
        sdk_name = outputs[0]
        row = f"|{sdk_name}|" + \
            "|".join(outputs[1:]) + "|"

        markdown += row

    return markdown


def test():
    s = """
    | sdk name | dev | release | dev all features | release all features |
    | -------- | --- | ------- | ---------------- | -------------------- |
    |iam|201.92|217.43|215.10|187.71|
    """.strip()
    s2 = parser(read_file()).strip()
    assert s2 == s, f"{s2} \n== \n{s}"


if __name__ == '__main__':
    test()
    main()
