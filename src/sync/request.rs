use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::error::{AruError, Result};
use crate::lockfile::Lockfile;
use crate::manifest::Manifest;
use crate::resolver::SkillResolutionHint;

use super::{SyncOptions, SyncResult, prepare};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CollisionPolicy {
    Reject,
    MergeInstructions,
    Force,
}

impl CollisionPolicy {
    pub(crate) fn from_flags(merge_instructions: bool, force: bool) -> Result<Self> {
        match (merge_instructions, force) {
            (false, false) => Ok(Self::Reject),
            (true, false) => Ok(Self::MergeInstructions),
            (false, true) => Ok(Self::Force),
            (true, true) => Err(AruError::msg("--merge and --force cannot be combined")),
        }
    }

    fn flags(self) -> (bool, bool) {
        match self {
            Self::Reject => (false, false),
            Self::MergeInstructions => (true, false),
            Self::Force => (false, true),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct UpdateSelection {
    skills: BTreeSet<String>,
    mcp: BTreeSet<String>,
    packages: BTreeSet<String>,
    precise_packages: BTreeMap<String, String>,
    skill_hints: BTreeMap<String, SkillResolutionHint>,
}

impl UpdateSelection {
    pub(crate) fn skills(mut self, skills: BTreeSet<String>) -> Self {
        self.skills = skills;
        self
    }

    pub(crate) fn mcp(mut self, mcp: BTreeSet<String>) -> Self {
        self.mcp = mcp;
        self
    }

    pub(crate) fn packages(
        mut self,
        packages: BTreeSet<String>,
        precise: BTreeMap<String, String>,
    ) -> Self {
        self.packages = packages;
        self.precise_packages = precise;
        self
    }

    pub(crate) fn plugins(
        mut self,
        plugins: BTreeSet<String>,
        precise: BTreeMap<String, String>,
    ) -> Self {
        self.packages.extend(plugins);
        self.precise_packages.extend(precise);
        self
    }

    pub(crate) fn skill_hints(
        mut self,
        skill_hints: BTreeMap<String, SkillResolutionHint>,
    ) -> Self {
        self.skill_hints = skill_hints;
        self
    }

    fn is_empty(&self) -> bool {
        self.skills.is_empty()
            && self.mcp.is_empty()
            && self.packages.is_empty()
            && self.precise_packages.is_empty()
            && self.skill_hints.is_empty()
    }
}

#[derive(Debug, Clone, Copy)]
enum ReconcileMode {
    LockOnly {
        materialize_skills: bool,
    },
    Project {
        materialize_skills: bool,
        collision: CollisionPolicy,
    },
}

#[derive(Debug)]
pub(crate) struct ReconcileRequest {
    mode: ReconcileMode,
    locked: bool,
    offline: bool,
    dry_run: bool,
    manifest_bytes: Option<Vec<u8>>,
    updates: UpdateSelection,
}

impl ReconcileRequest {
    pub(crate) fn lock_update(locked: bool, offline: bool, dry_run: bool) -> Self {
        Self {
            mode: ReconcileMode::LockOnly {
                materialize_skills: true,
            },
            locked,
            offline,
            dry_run,
            manifest_bytes: None,
            updates: UpdateSelection::default(),
        }
    }

    pub(crate) fn project_update(
        locked: bool,
        offline: bool,
        dry_run: bool,
        collision: CollisionPolicy,
    ) -> Self {
        Self {
            mode: ReconcileMode::Project {
                materialize_skills: true,
                collision,
            },
            locked,
            offline,
            dry_run,
            manifest_bytes: None,
            updates: UpdateSelection::default(),
        }
    }

    pub(crate) fn check_lock() -> Self {
        Self::check(false)
    }

    pub(crate) fn check_project() -> Self {
        Self::check(true)
    }

    fn check(project: bool) -> Self {
        Self {
            mode: if project {
                ReconcileMode::Project {
                    materialize_skills: false,
                    collision: CollisionPolicy::Reject,
                }
            } else {
                ReconcileMode::LockOnly {
                    materialize_skills: false,
                }
            },
            locked: true,
            offline: true,
            dry_run: true,
            manifest_bytes: None,
            updates: UpdateSelection::default(),
        }
    }

    pub(crate) fn with_manifest_bytes(mut self, manifest_bytes: Vec<u8>) -> Self {
        self.manifest_bytes = Some(manifest_bytes);
        self
    }

    pub(crate) fn with_updates(mut self, updates: UpdateSelection) -> Self {
        self.updates = updates;
        self
    }

    pub(crate) fn dry_run(&self) -> bool {
        self.dry_run
    }

    pub(crate) fn locked(&self) -> bool {
        self.locked
    }

    pub(crate) fn projects(&self) -> bool {
        matches!(self.mode, ReconcileMode::Project { .. })
    }

    pub(crate) fn changes_intent(&self) -> bool {
        self.manifest_bytes.is_some() || !self.updates.is_empty()
    }
}

pub(crate) fn prepare_request(
    project: &Path,
    manifest: &Manifest,
    previous: Option<&Lockfile>,
    request: ReconcileRequest,
) -> Result<SyncResult> {
    let (materialize_skills, project_projections, merge_instructions, force) = match request.mode {
        ReconcileMode::LockOnly { materialize_skills } => (materialize_skills, false, false, false),
        ReconcileMode::Project {
            materialize_skills,
            collision,
        } => {
            let (merge_instructions, force) = collision.flags();
            (materialize_skills, true, merge_instructions, force)
        }
    };
    prepare(
        project,
        manifest,
        SyncOptions {
            previous,
            locked: request.locked,
            offline: request.offline,
            materialize_skills,
            dry_run: request.dry_run,
            project_projections,
            force,
            merge_instructions,
            manifest_bytes: request.manifest_bytes,
            update_skills: &request.updates.skills,
            update_mcp: &request.updates.mcp,
            update_packages: &request.updates.packages,
            precise_packages: &request.updates.precise_packages,
            skill_hints: &request.updates.skill_hints,
        },
    )
}
