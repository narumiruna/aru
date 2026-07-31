use std::collections::BTreeSet;
use std::path::Path;

use aru::lockfile::Lockfile;
use aru::manifest::{Manifest, SkillRequirement};
use aru::resolver::{
    SkillResolutionHint, SkillSourceInspection, canonical_update_skill_targets,
    inspect_skill_source,
};

type InspectSkillSource = fn(
    &Path,
    &str,
    &SkillRequirement,
    Option<&Lockfile>,
    bool,
    bool,
) -> aru::Result<SkillSourceInspection>;
type CanonicalUpdateSkillTargets = fn(&Path, &Manifest, &[String]) -> aru::Result<BTreeSet<String>>;

#[test]
fn direct_skill_resolver_exports_remain_available() {
    let _: InspectSkillSource = inspect_skill_source;
    let _: CanonicalUpdateSkillTargets = canonical_update_skill_targets;
    let hint = SkillResolutionHint {
        requirement: "version:*".into(),
        version: "1.0.0".into(),
        revision: "0123456789abcdef0123456789abcdef01234567".into(),
    };

    assert_eq!(hint.version, "1.0.0");
}
