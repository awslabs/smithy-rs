/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use anyhow::Context;
use clap::Parser;
use smithy_rs_tool_common::changelog::{FrontMatter, Markdown, Reference, Target};
use smithy_rs_tool_common::git::find_git_repository_root;
use smithy_rs_tool_common::here;
use std::path::PathBuf;

#[derive(Parser, Debug, Eq, PartialEq)]
pub struct NewEntryArgs {
    /// Target audience for the change
    #[clap(long)]
    pub applies_to: Option<Vec<Target>>,
    /// List of git usernames for the authors of the change
    #[clap(long = "author")]
    pub authors: Option<Vec<String>>,
    /// List of relevant issues and PRs
    #[clap(long = "ref")]
    pub references: Option<Vec<Reference>>,
    /// Whether or not the change contains a breaking change (defaults to false)
    #[clap(long, action)]
    pub breaking: bool,
    /// Whether or not the change implements a new feature (defaults to false)
    #[clap(long, action)]
    pub new_feature: bool,
    /// Whether or not the change fixes a bug (defaults to false)
    #[clap(long, action)]
    pub bug_fix: bool,
    /// The changelog entry message
    #[clap(long)]
    pub message: Option<String>,
    /// Basename of a changelog markdown file (defaults to a random 6-digit basename)
    #[clap(long)]
    pub basename: Option<PathBuf>,
}

pub fn subcommand_new_entry(args: NewEntryArgs) -> anyhow::Result<()> {
    let mut md_full_filename = find_git_repository_root("smithy-rs", ".").context(here!())?;
    md_full_filename.push(".changelog");
    md_full_filename.push(args.basename.clone().unwrap_or(PathBuf::from(format!(
        "{}.md",
        fastrand::u32(1_000_000..10_000_000)
    ))));

    let changelog_entry = new_entry(args)?;
    std::fs::write(&md_full_filename, &changelog_entry).with_context(|| {
        format!(
            "failed to write the following changelog entry to {:?}:\n{}",
            md_full_filename.as_path(),
            changelog_entry
        )
    })?;

    println!(
        "\nThe following changelog entry has been written to {:?}:\n{}",
        md_full_filename.as_path(),
        changelog_entry
    );

    Ok(())
}

fn new_entry(args: NewEntryArgs) -> anyhow::Result<String> {
    let markdown = Markdown {
        front_matter: FrontMatter {
            applies_to: args.applies_to.unwrap_or_default().into_iter().collect(),
            authors: args.authors.unwrap_or_default(),
            references: args.references.unwrap_or_default(),
            breaking: args.breaking,
            new_feature: args.new_feature,
            bug_fix: args.bug_fix,
        },
        message: args.message.unwrap_or_default(),
    };
    // Due to the inability for `serde_yaml` to output single line array syntax, an array of values
    // will be serialized as follows:
    //
    // key:
    // - value1
    // - value2
    //
    // as opposed to:
    //
    // key: [value1, value2]
    //
    // This doesn't present practical issues when rendering changelogs. See
    // https://github.com/dtolnay/serde-yaml/issues/355
    let front_matter = serde_yaml::to_string(&markdown.front_matter)?;
    let changelog_entry = format!("---\n{}---\n{}", front_matter, markdown.message);
    let changelog_entry = if any_required_field_needs_to_be_filled(&markdown) {
        edit::edit(changelog_entry).context("failed while editing changelog entry)")?
    } else {
        changelog_entry
    };

    Ok(changelog_entry)
}

fn any_required_field_needs_to_be_filled(markdown: &Markdown) -> bool {
    macro_rules! any_empty {
        () => { false };
        ($head:expr $(, $tail:expr)*) => {
            $head.is_empty() || any_empty!($($tail),*)
        };
    }
    any_empty!(
        &markdown.front_matter.applies_to,
        &markdown.front_matter.authors,
        &markdown.front_matter.references,
        &markdown.message
    )
}

#[cfg(test)]
mod tests {
    use crate::new_entry::{new_entry, NewEntryArgs};
    use smithy_rs_tool_common::changelog::{Reference, Target};
    use std::str::FromStr;

    #[test]
    fn test_new_entry_from_args() {
        // make sure `args` populates required fields (so the function
        // `any_required_field_needs_to_be_filled` returns true), otherwise an editor would be
        // opened during the test execution for human input, causing the test to get struck
        let args = NewEntryArgs {
            applies_to: Some(vec![Target::Client]),
            authors: Some(vec!["ysaito1001".to_owned()]),
            references: Some(vec![Reference::from_str("smithy-rs#1234").unwrap()]),
            breaking: false,
            new_feature: true,
            bug_fix: false,
            message: Some("Implement a long-awaited feature for S3".to_owned()),
            basename: None,
        };

        let expected = "---\napplies_to:\n- client\nauthors:\n- ysaito1001\nreferences:\n- smithy-rs#1234\nbreaking: false\nnew_feature: true\nbug_fix: false\n---\nImplement a long-awaited feature for S3";
        let actual = new_entry(args).unwrap();

        assert_eq!(expected, &actual);
    }
}
