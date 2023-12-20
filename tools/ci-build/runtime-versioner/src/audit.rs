/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::{
    index::CratesIndex,
    repo::Repo,
    tag::{previous_release_tag, release_tags},
    util::utf8_path_buf,
    Audit,
};
use anyhow::{anyhow, bail, Context, Result};
use camino::Utf8PathBuf;
use smithy_rs_tool_common::release_tag::ReleaseTag;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

pub fn audit(args: Audit) -> Result<()> {
    let repo = Repo::new(args.smithy_rs_path.as_deref())?;
    if !args.no_fetch {
        // Make sure we have the latest release tags
        fetch_smithy_rs_tags(&repo)?;
    }

    let release_tags = release_tags(&repo)?;
    let previous_release_tag =
        previous_release_tag(&repo, &release_tags, args.previous_release_tag.as_deref())?;
    if release_tags.first() != Some(&previous_release_tag) {
        tracing::warn!("there are newer releases since '{previous_release_tag}'");
    }

    let next_commit_hash = current_head(&repo)?;
    let next_crates = discover_runtime_crates(&repo).context("next")?;

    checkout(&repo, previous_release_tag.as_str()).context("previous")?;
    let previous_crates = discover_runtime_crates(&repo).context("previous")?;
    checkout(&repo, &next_commit_hash).context("next")?;

    let crates = augment_runtime_crates(previous_crates, next_crates, args.fake_crates_io_index)?;
    let mut errors = Vec::new();
    for rt_crate in crates {
        if let Err(err) = audit_crate(&repo, &previous_release_tag, rt_crate) {
            errors.push(err);
        }
    }
    if errors.is_empty() {
        println!("SUCCESS");
        Ok(())
    } else {
        for error in errors {
            eprintln!("{error}");
        }
        Err(anyhow!("there are audit failures in the runtime crates"))
    }
}

fn audit_crate(repo: &Repo, release_tag: &ReleaseTag, rt_crate: RuntimeCrate) -> Result<()> {
    if rt_crate.changed_since_release(repo, release_tag)? {
        // If this version has never been published before, then we're good.
        // (This tool doesn't check semver compatibility.)
        if !rt_crate.next_version_is_published() {
            if let Some(previous_version) = rt_crate.previous_release_version {
                tracing::info!(
                    "'{}' changed and was version bumped from {previous_version} to {}",
                    rt_crate.name,
                    rt_crate.next_release_version,
                );
            } else {
                tracing::info!(
                    "'{}' is a new crate (or wasn't independently versioned before) and will publish at {}",
                    rt_crate.name,
                    rt_crate.next_release_version,
                );
            }
            Ok(())
        } else if rt_crate.previous_release_version.as_ref() != Some(&rt_crate.next_release_version)
        {
            Err(anyhow!(
                "{crate_name} was changed and version bumped, but the new version \
                number ({version}) has already been published to crates.io. Choose a new \
                version number.",
                crate_name = rt_crate.name,
                version = rt_crate.next_release_version,
            ))
        } else {
            Err(anyhow!(
                "{crate_name} changed since {release_tag} and requires a version bump",
                crate_name = rt_crate.name
            ))
        }
    } else {
        // If it didn't change at all since last release, then we're good.
        Ok(())
    }
}

struct RuntimeCrate {
    name: String,
    path: Utf8PathBuf,
    previous_release_version: Option<String>,
    next_release_version: String,
    published_versions: Vec<String>,
}

impl RuntimeCrate {
    /// True if the runtime crate's next version exists in crates.io
    fn next_version_is_published(&self) -> bool {
        self.published_versions
            .iter()
            .any(|version| self.next_release_version == *version)
    }

    /// True if this runtime crate changed since the given release tag.
    fn changed_since_release(&self, repo: &Repo, release_tag: &ReleaseTag) -> Result<bool> {
        let status = repo
            .git(["diff", "--quiet", release_tag.as_str(), self.path.as_str()])
            .status()
            .with_context(|| format!("failed to git diff {}", self.name))?;
        match status.code() {
            Some(0) => Ok(false),
            Some(1) => Ok(true),
            code => bail!("unknown git diff result: {code:?}"),
        }
    }
}

/// Loads version information from crates.io and attaches it to the passed in runtime crates.
fn augment_runtime_crates(
    previous_crates: BTreeMap<String, DiscoveredCrate>,
    next_crates: BTreeMap<String, DiscoveredCrate>,
    fake_crates_io_index: Option<Utf8PathBuf>,
) -> Result<Vec<RuntimeCrate>> {
    let index = fake_crates_io_index
        .map(CratesIndex::fake)
        .map(Ok)
        .unwrap_or_else(CratesIndex::real)?;
    let all_keys: BTreeSet<_> = previous_crates.keys().chain(next_crates.keys()).collect();
    let mut result = Vec::new();
    for key in all_keys {
        let previous_crate = previous_crates.get(key);
        if let Some(next_crate) = next_crates.get(key) {
            result.push(RuntimeCrate {
                published_versions: index.published_versions(&next_crate.name)?,
                name: next_crate.name.clone(),
                previous_release_version: previous_crate.map(|c| c.version.clone()),
                next_release_version: next_crate.version.clone(),
                path: next_crate.path.clone(),
            });
        } else {
            tracing::warn!("runtime crate '{key}' was removed and will not be published");
        }
    }
    Ok(result)
}

struct DiscoveredCrate {
    name: String,
    version: String,
    path: Utf8PathBuf,
}

/// Discovers runtime crates that are independently versioned.
/// For now, that just means the ones that don't have the special version number `0.0.0-smithy-rs-head`.
/// In the future, this can be simplified to just return all the runtime crates.
fn discover_runtime_crates(repo: &Repo) -> Result<BTreeMap<String, DiscoveredCrate>> {
    const ROOT_PATHS: &[&str] = &["rust-runtime", "aws/rust-runtime"];
    let mut result = BTreeMap::new();
    for &root in ROOT_PATHS {
        let root = repo.root.join(root);
        for entry in fs::read_dir(&root).context("failed to read dir")? {
            let entry = entry.context("failed to read dir entry")?;
            if !entry.path().is_dir() {
                continue;
            }
            let manifest_path = entry.path().join("Cargo.toml");
            if !manifest_path.exists() {
                continue;
            }
            let manifest: toml::Value =
                toml::from_slice(&fs::read(&manifest_path).context("failed to read manifest")?)
                    .context("failed to parse manifest")?;
            let publish = manifest["package"]
                .get("publish")
                .and_then(|p| p.as_bool())
                .unwrap_or(true);
            let version = manifest["package"]["version"]
                .as_str()
                .expect("version is a string");
            if publish && version != "0.0.0-smithy-rs-head" {
                let name: String = entry.path().file_name().unwrap().to_string_lossy().into();
                result.insert(
                    name.clone(),
                    DiscoveredCrate {
                        name,
                        version: version.into(),
                        path: utf8_path_buf(entry.path()),
                    },
                );
            }
        }
    }
    Ok(result)
}

/// Fetches the latest tags from smithy-rs origin.
fn fetch_smithy_rs_tags(repo: &Repo) -> Result<()> {
    let output = repo
        .git(["remote", "get-url", "origin"])
        .output()
        .context("failed to verify origin git remote")?;
    let origin_url = String::from_utf8(output.stdout).expect("valid utf-8");
    if origin_url != "git@github.com:smithy-lang/smithy-rs.git" {
        bail!("smithy-rs origin must be 'git@github.com:smithy-lang/smithy-rs.git' in order to get the latest release tags");
    }

    let status = repo
        .git(["fetch", "--tags", "origin"])
        .status()
        .context("failed to fetch tags")?;
    if !status.success() {
        bail!("failed to fetch tags");
    }
    Ok(())
}

/// Returns the current HEAD commit hash.
fn current_head(repo: &Repo) -> Result<String> {
    let output = repo
        .git(["rev-parse", "HEAD"])
        .output()
        .context("failed to retrieve current commit hash")?;
    let hash = String::from_utf8(output.stdout).expect("valid utf-8");
    Ok(hash.trim_end_matches('\n').into())
}

fn checkout(repo: &Repo, revision: &str) -> Result<()> {
    let output = repo
        .git(["checkout", revision])
        .output()
        .context("failed to git checkout")?;
    if !output.status.success() {
        bail!("failed to git checkout");
    }
    Ok(())
}
