/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::lint::LintError;
use crate::{repo_root, Check, Lint};
use anyhow::Result;
use smithy_rs_tool_common::changelog::{ChangelogLoader, ValidationSet};
use std::path::{Path, PathBuf};

pub(crate) struct ChangelogNext;

impl Lint for ChangelogNext {
    fn name(&self) -> &str {
        "Changelog.next"
    }

    fn files_to_check(&self) -> Result<Vec<PathBuf>> {
        Ok(vec![repo_root().join("CHANGELOG.next.toml")])
    }
}

impl Check for ChangelogNext {
    fn check(&self, path: impl AsRef<Path>) -> Result<Vec<LintError>> {
        match check_changelog_next(path) {
            Ok(_) => Ok(vec![]),
            Err(errs) => Ok(errs),
        }
    }
}

// TODO(file-per-change-changelog): Use `.load_from_dir` to read from the `.changelog` directory
//  and run the validation only when the directory has at least one changelog entry file, otherwise
//  a default constructed `ChangeLog` won't pass the validation.
/// Validate that `CHANGELOG.next.toml` follows best practices
fn check_changelog_next(path: impl AsRef<Path>) -> std::result::Result<(), Vec<LintError>> {
    let parsed = ChangelogLoader::default()
        .load_from_file(path)
        .map_err(|e| vec![LintError::via_display(e)])?;
    parsed
        .validate(ValidationSet::Development)
        .map_err(|errs| {
            errs.into_iter()
                .map(LintError::via_display)
                .collect::<Vec<_>>()
        })?;
    Ok(())
}
