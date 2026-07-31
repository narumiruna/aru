use std::path::{Path, PathBuf};

use crate::cli::PackageArchiveArgs;
use crate::error::Result;
use crate::package::archive;

use super::ExecutionPolicy;

pub fn run(root: &Path, args: PackageArchiveArgs, policy: ExecutionPolicy) -> Result<()> {
    let requested_output = args.output.as_ref().map(|path| absolute_output(root, path));
    let input = archive::collect(root, requested_output.as_deref(), args.allow_dirty)?;
    if input.dirty {
        policy
            .output
            .warning("Packaging a dirty Git worktree because --allow-dirty was provided.");
    }
    let snapshot = archive::snapshot(&input.entries)?;
    crate::package::resolver::validate_archive_graph(
        snapshot.root(),
        &input.manifest,
        policy.offline,
    )?;
    if args.list {
        for entry in input.entries {
            println!("{}", entry.path);
        }
        return Ok(());
    }
    let output = requested_output.unwrap_or_else(|| {
        root.join("target/aru-package").join(format!(
            "{}-{}.aru-package.tar.gz",
            input.manifest.package.name, input.manifest.package.version
        ))
    });
    archive::validate_output_path(root, &output)?;
    let bytes = archive::bytes(&input.entries)?;
    archive::write_atomic(&output, &bytes)?;
    policy.output.completion(&format!(
        "Packaged {} v{} at {}",
        input.manifest.package.name,
        input.manifest.package.version,
        output.display()
    ));
    Ok(())
}

fn absolute_output(root: &Path, output: &Path) -> PathBuf {
    if output.is_absolute() {
        output.into()
    } else {
        root.join(output)
    }
}
