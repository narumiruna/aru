use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use flate2::read::GzDecoder;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::error::{AruError, IoContext, Result};

const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 100 * 1024 * 1024;
const MAX_BINARY_BYTES: u64 = 100 * 1024 * 1024;

pub(crate) fn is_standalone_build() -> bool {
    option_env!("ARU_BUILD_DISTRIBUTION") == Some("standalone")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    TarGz,
    Zip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReleaseTarget {
    triple: &'static str,
    binary_name: &'static str,
    archive_kind: ArchiveKind,
}

impl ReleaseTarget {
    fn current() -> Result<Self> {
        Self::for_platform(std::env::consts::OS, std::env::consts::ARCH)
    }

    fn for_platform(os: &str, architecture: &str) -> Result<Self> {
        match (os, architecture) {
            ("linux", "x86_64") => Ok(Self {
                triple: "x86_64-unknown-linux-musl",
                binary_name: "aru",
                archive_kind: ArchiveKind::TarGz,
            }),
            ("macos", "x86_64") => Ok(Self {
                triple: "x86_64-apple-darwin",
                binary_name: "aru",
                archive_kind: ArchiveKind::TarGz,
            }),
            ("macos", "aarch64") => Ok(Self {
                triple: "aarch64-apple-darwin",
                binary_name: "aru",
                archive_kind: ArchiveKind::TarGz,
            }),
            ("windows", "x86_64") => Ok(Self {
                triple: "x86_64-pc-windows-msvc",
                binary_name: "aru.exe",
                archive_kind: ArchiveKind::Zip,
            }),
            _ => Err(AruError::msg(format!(
                "unsupported self-update platform: {os} {architecture}"
            ))),
        }
    }

    fn archive_name(self, version: &Version) -> String {
        let extension = match self.archive_kind {
            ArchiveKind::TarGz => "tar.gz",
            ArchiveKind::Zip => "zip",
        };
        format!("aru-{version}-{}.{extension}", self.triple)
    }
}

#[derive(Debug, Clone)]
struct ReleaseSource {
    latest_url: String,
    download_base_url: String,
}

impl ReleaseSource {
    fn production() -> Self {
        Self {
            latest_url: "https://api.github.com/repos/narumiruna/aru/releases/latest".into(),
            download_base_url: "https://github.com/narumiruna/aru/releases/download".into(),
        }
    }

    fn archive_url(&self, version: &Version, archive: &str) -> String {
        format!("{}/v{version}/{archive}", self.download_base_url)
    }

    fn checksums_url(&self, version: &Version) -> String {
        format!("{}/v{version}/SHA256SUMS", self.download_base_url)
    }
}

#[derive(Debug)]
struct UpdateContext {
    standalone: bool,
    offline: bool,
    dry_run: bool,
    current_version: Version,
    target: ReleaseTarget,
    source: ReleaseSource,
    executable: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateAction {
    UpToDate,
    LocalNewer,
    WouldUpdate,
    Updated,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UpdateOutcome {
    pub(crate) action: UpdateAction,
    pub(crate) current_version: Version,
    pub(crate) latest_version: Version,
    pub(crate) executable: PathBuf,
}

trait Fetcher {
    fn fetch(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>>;
}

struct HttpFetcher {
    client: reqwest::blocking::Client,
}

impl HttpFetcher {
    fn new() -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 3 {
                    attempt.error("self-update redirect limit exceeded")
                } else if attempt.url().scheme() != "https"
                    || !attempt.url().username().is_empty()
                    || attempt.url().password().is_some()
                {
                    attempt.error("self-update redirect must remain credential-free HTTPS")
                } else {
                    attempt.follow()
                }
            }))
            .user_agent(concat!("aru/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { client })
    }
}

impl Fetcher for HttpFetcher {
    fn fetch(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>> {
        validate_download_url(url)?;
        let response = self.client.get(url).send()?.error_for_status()?;
        let content_length = response.content_length();
        read_bounded_response(response, content_length, max_bytes)
    }
}

trait Replacer {
    fn replace(&self, new_executable: &Path) -> Result<()>;
}

struct SystemReplacer;

impl Replacer for SystemReplacer {
    fn replace(&self, new_executable: &Path) -> Result<()> {
        self_replace::self_replace(new_executable)
            .map_err(|error| AruError::msg(format!("failed to replace aru executable: {error}")))
    }
}

pub(crate) fn update(dry_run: bool, offline: bool) -> Result<UpdateOutcome> {
    if !is_standalone_build() {
        return Err(standalone_error());
    }
    if offline {
        return Err(AruError::msg("self-update is unavailable in offline mode"));
    }
    let context = UpdateContext {
        standalone: true,
        offline: false,
        dry_run,
        current_version: Version::parse(env!("CARGO_PKG_VERSION"))
            .map_err(|error| AruError::msg(format!("invalid current aru version: {error}")))?,
        target: ReleaseTarget::current()?,
        source: ReleaseSource::production(),
        executable: std::env::current_exe()
            .map_err(|error| AruError::msg(format!("could not locate aru executable: {error}")))?,
    };
    execute_with(context, &HttpFetcher::new()?, &SystemReplacer)
}

fn execute_with(
    context: UpdateContext,
    fetcher: &dyn Fetcher,
    replacer: &dyn Replacer,
) -> Result<UpdateOutcome> {
    if !context.standalone {
        return Err(standalone_error());
    }
    if context.offline {
        return Err(AruError::msg("self-update is unavailable in offline mode"));
    }

    let latest_version = latest_version(fetcher, &context.source)?;
    let action = if latest_version == context.current_version {
        UpdateAction::UpToDate
    } else if latest_version < context.current_version {
        UpdateAction::LocalNewer
    } else {
        let archive_name = context.target.archive_name(&latest_version);
        let checksums = fetcher.fetch(
            &context.source.checksums_url(&latest_version),
            MAX_METADATA_BYTES,
        )?;
        let expected_checksum = expected_checksum(&checksums, &archive_name)?;
        let archive = fetcher.fetch(
            &context.source.archive_url(&latest_version, &archive_name),
            MAX_ARCHIVE_BYTES,
        )?;
        let actual_checksum = hex::encode(Sha256::digest(&archive));
        if actual_checksum != expected_checksum {
            return Err(AruError::msg(format!(
                "checksum verification failed for {archive_name}"
            )));
        }

        let temporary = tempfile::tempdir()
            .map_err(|error| AruError::msg(format!("could not stage aru update: {error}")))?;
        let staged = temporary.path().join(context.target.binary_name);
        extract_binary(&archive, context.target, &staged)?;
        if context.dry_run {
            UpdateAction::WouldUpdate
        } else {
            replacer.replace(&staged)?;
            UpdateAction::Updated
        }
    };

    Ok(UpdateOutcome {
        action,
        current_version: context.current_version,
        latest_version,
        executable: context.executable,
    })
}

fn standalone_error() -> AruError {
    AruError::msg(
        "self-update is only available for standalone aru installations; update this installation with `cargo install aru --locked`",
    )
}

#[derive(Debug, Deserialize)]
struct LatestRelease {
    tag_name: String,
}

fn latest_version(fetcher: &dyn Fetcher, source: &ReleaseSource) -> Result<Version> {
    let bytes = fetcher.fetch(&source.latest_url, MAX_METADATA_BYTES)?;
    let release: LatestRelease = serde_json::from_slice(&bytes)
        .map_err(|error| AruError::msg(format!("invalid GitHub release metadata: {error}")))?;
    let raw = release
        .tag_name
        .strip_prefix('v')
        .ok_or_else(|| AruError::msg("latest release tag must match vX.Y.Z"))?;
    let version =
        Version::parse(raw).map_err(|_| AruError::msg("latest release tag must match vX.Y.Z"))?;
    if !version.pre.is_empty()
        || !version.build.is_empty()
        || release.tag_name != format!("v{version}")
    {
        return Err(AruError::msg("latest release tag must match vX.Y.Z"));
    }
    Ok(version)
}

fn expected_checksum(bytes: &[u8], archive_name: &str) -> Result<String> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| AruError::msg("SHA256SUMS is not valid UTF-8"))?;
    let mut found = None;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(checksum) = fields.next() else {
            continue;
        };
        let Some(name) = fields.next() else {
            continue;
        };
        if fields.next().is_some() || name.strip_prefix('*').unwrap_or(name) != archive_name {
            continue;
        }
        if found.is_some() {
            return Err(AruError::msg(format!(
                "SHA256SUMS contains duplicate entries for {archive_name}"
            )));
        }
        if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(AruError::msg(format!(
                "SHA256SUMS has an invalid checksum for {archive_name}"
            )));
        }
        found = Some(checksum.to_ascii_lowercase());
    }
    found.ok_or_else(|| AruError::msg(format!("SHA256SUMS has no checksum for {archive_name}")))
}

fn extract_binary(archive: &[u8], target: ReleaseTarget, destination: &Path) -> Result<()> {
    match target.archive_kind {
        ArchiveKind::TarGz => extract_tar_gz(archive, target.binary_name, destination),
        ArchiveKind::Zip => extract_zip(archive, target.binary_name, destination),
    }
}

fn extract_tar_gz(archive: &[u8], binary_name: &str, destination: &Path) -> Result<()> {
    let decoder = GzDecoder::new(Cursor::new(archive));
    let mut archive = tar::Archive::new(decoder);
    let mut entries = archive
        .entries()
        .map_err(|error| AruError::msg(format!("invalid release archive: {error}")))?;
    let mut entry = entries
        .next()
        .transpose()
        .map_err(|error| AruError::msg(format!("invalid release archive: {error}")))?
        .ok_or_else(|| AruError::msg("release archive has unexpected contents"))?;
    if !entry.header().entry_type().is_file()
        || entry
            .path()
            .map_err(|error| AruError::msg(format!("invalid release archive path: {error}")))?
            != Path::new(binary_name)
        || entry.size() > MAX_BINARY_BYTES
    {
        return Err(AruError::msg("release archive has unexpected contents"));
    }
    write_bounded(&mut entry, destination)?;
    drop(entry);
    if entries.next().is_some() {
        return Err(AruError::msg("release archive has unexpected contents"));
    }
    Ok(())
}

fn extract_zip(archive: &[u8], binary_name: &str, destination: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(archive))
        .map_err(|error| AruError::msg(format!("invalid release archive: {error}")))?;
    if archive.len() != 1 {
        return Err(AruError::msg("release archive has unexpected contents"));
    }
    let mut entry = archive
        .by_index(0)
        .map_err(|error| AruError::msg(format!("invalid release archive: {error}")))?;
    if entry.is_dir() || entry.name() != binary_name || entry.size() > MAX_BINARY_BYTES {
        return Err(AruError::msg("release archive has unexpected contents"));
    }
    write_bounded(&mut entry, destination)
}

fn write_bounded(reader: &mut dyn Read, destination: &Path) -> Result<()> {
    let mut file = File::create(destination).at(destination)?;
    let written =
        std::io::copy(&mut reader.take(MAX_BINARY_BYTES + 1), &mut file).at(destination)?;
    if written > MAX_BINARY_BYTES || written == 0 {
        return Err(AruError::msg("release archive binary has an invalid size"));
    }
    file.flush().at(destination)?;
    file.sync_all().at(destination)?;
    Ok(())
}

fn read_bounded_response(
    mut reader: impl Read,
    content_length: Option<u64>,
    max_bytes: u64,
) -> Result<Vec<u8>> {
    if content_length.is_some_and(|length| length > max_bytes) {
        return Err(AruError::msg(format!(
            "self-update response exceeds {max_bytes} bytes"
        )));
    }
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| AruError::msg(format!("failed to read self-update response: {error}")))?;
    if bytes.len() as u64 > max_bytes {
        return Err(AruError::msg(format!(
            "self-update response exceeds {max_bytes} bytes"
        )));
    }
    Ok(bytes)
}

fn validate_download_url(value: &str) -> Result<()> {
    let url = url::Url::parse(value)
        .map_err(|error| AruError::msg(format!("invalid self-update URL: {error}")))?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err(AruError::msg(
            "self-update URL must be credential-free HTTPS",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::{Cursor, Write};
    use std::sync::Mutex;

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use sha2::{Digest, Sha256};
    use tar::{Builder, EntryType, Header};
    use tempfile::TempDir;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;

    #[derive(Default)]
    struct FakeFetcher {
        responses: BTreeMap<String, Vec<u8>>,
        requests: Mutex<Vec<(String, u64)>>,
    }

    impl Fetcher for FakeFetcher {
        fn fetch(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>> {
            self.requests
                .lock()
                .unwrap()
                .push((url.to_owned(), max_bytes));
            self.responses
                .get(url)
                .cloned()
                .ok_or_else(|| AruError::msg(format!("test fetch failed for {url}")))
        }
    }

    struct FakeReplacer {
        destination: PathBuf,
        fail: bool,
        calls: Mutex<usize>,
    }

    impl FakeReplacer {
        fn new(destination: PathBuf) -> Self {
            Self {
                destination,
                fail: false,
                calls: Mutex::new(0),
            }
        }

        fn failing(destination: PathBuf) -> Self {
            Self {
                destination,
                fail: true,
                calls: Mutex::new(0),
            }
        }

        fn calls(&self) -> usize {
            *self.calls.lock().unwrap()
        }
    }

    impl Replacer for FakeReplacer {
        fn replace(&self, new_executable: &Path) -> Result<()> {
            *self.calls.lock().unwrap() += 1;
            if self.fail {
                return Err(AruError::msg("replacement permission denied"));
            }
            std::fs::copy(new_executable, &self.destination)
                .map(|_| ())
                .map_err(|error| AruError::msg(error.to_string()))
        }
    }

    fn linux_target() -> ReleaseTarget {
        ReleaseTarget {
            triple: "x86_64-unknown-linux-musl",
            binary_name: "aru",
            archive_kind: ArchiveKind::TarGz,
        }
    }

    fn context(temporary: &TempDir, dry_run: bool) -> UpdateContext {
        let executable = temporary.path().join("aru");
        std::fs::write(&executable, b"old binary").unwrap();
        UpdateContext {
            standalone: true,
            offline: false,
            dry_run,
            current_version: Version::parse("1.0.0").unwrap(),
            target: linux_target(),
            source: ReleaseSource {
                latest_url: "https://updates.test/latest".into(),
                download_base_url: "https://updates.test/download".into(),
            },
            executable,
        }
    }

    fn tar_gz(entries: &[(&str, &[u8], EntryType)]) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut archive = Builder::new(encoder);
        for (path, body, entry_type) in entries {
            let mut header = Header::new_gnu();
            header.set_entry_type(*entry_type);
            header.set_mode(0o755);
            header.set_size(body.len() as u64);
            header.set_cksum();
            archive
                .append_data(&mut header, path, Cursor::new(*body))
                .unwrap();
        }
        archive.into_inner().unwrap().finish().unwrap()
    }

    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        for (path, body) in entries {
            archive
                .start_file(*path, SimpleFileOptions::default())
                .unwrap();
            archive.write_all(body).unwrap();
        }
        archive.finish().unwrap().into_inner()
    }

    fn checksum(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    fn updater(
        context: &UpdateContext,
        archive: Vec<u8>,
        checksum_override: Option<&str>,
    ) -> FakeFetcher {
        let latest = Version::parse("1.1.0").unwrap();
        let archive_name = format!(
            "aru-{latest}-{}.{}",
            context.target.triple,
            match context.target.archive_kind {
                ArchiveKind::TarGz => "tar.gz",
                ArchiveKind::Zip => "zip",
            }
        );
        let checksum = checksum_override
            .map(str::to_owned)
            .unwrap_or_else(|| checksum(&archive));
        let mut responses = BTreeMap::new();
        responses.insert(
            context.source.latest_url.clone(),
            br#"{"tag_name":"v1.1.0"}"#.to_vec(),
        );
        responses.insert(
            context.source.checksums_url(&latest),
            format!("{checksum}  {archive_name}\n").into_bytes(),
        );
        responses.insert(context.source.archive_url(&latest, &archive_name), archive);
        FakeFetcher {
            responses,
            requests: Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn release_target_mapping_matches_published_assets() {
        assert_eq!(
            ReleaseTarget::for_platform("linux", "x86_64").unwrap(),
            linux_target()
        );
        assert_eq!(
            ReleaseTarget::for_platform("macos", "aarch64").unwrap(),
            ReleaseTarget {
                triple: "aarch64-apple-darwin",
                binary_name: "aru",
                archive_kind: ArchiveKind::TarGz,
            }
        );
        assert_eq!(
            ReleaseTarget::for_platform("windows", "x86_64").unwrap(),
            ReleaseTarget {
                triple: "x86_64-pc-windows-msvc",
                binary_name: "aru.exe",
                archive_kind: ArchiveKind::Zip,
            }
        );
        assert!(
            ReleaseTarget::for_platform("linux", "aarch64")
                .unwrap_err()
                .to_string()
                .contains("unsupported self-update platform")
        );
    }

    #[test]
    fn bounded_response_reader_rejects_declared_and_streamed_overflow() {
        for content_length in [Some(5), None] {
            let error =
                read_bounded_response(Cursor::new(vec![0_u8; 5]), content_length, 4).unwrap_err();
            assert!(error.to_string().contains("exceeds 4 bytes"), "{error}");
        }
        assert_eq!(
            read_bounded_response(Cursor::new(b"aru"), Some(3), 4).unwrap(),
            b"aru"
        );
    }

    #[test]
    fn http_fetcher_rejects_insecure_or_credentialed_urls_before_network_access() {
        let fetcher = HttpFetcher::new().unwrap();
        for url in [
            "http://updates.example/latest",
            "https://token@updates.example/latest",
        ] {
            let error = fetcher.fetch(url, MAX_METADATA_BYTES).unwrap_err();
            assert!(
                error.to_string().contains("credential-free HTTPS"),
                "{error}"
            );
        }
    }

    #[test]
    fn verified_update_replaces_the_binary() {
        let temporary = tempfile::tempdir().unwrap();
        let context = context(&temporary, false);
        let archive = tar_gz(&[("aru", b"new binary", EntryType::Regular)]);
        let fetcher = updater(&context, archive, None);
        let replacer = FakeReplacer::new(context.executable.clone());

        let outcome = execute_with(context, &fetcher, &replacer).unwrap();

        assert_eq!(outcome.action, UpdateAction::Updated);
        assert_eq!(outcome.current_version, Version::parse("1.0.0").unwrap());
        assert_eq!(outcome.latest_version, Version::parse("1.1.0").unwrap());
        assert_eq!(replacer.calls(), 1);
        assert_eq!(std::fs::read(outcome.executable).unwrap(), b"new binary");
        assert_eq!(
            fetcher.requests.lock().unwrap().as_slice(),
            [
                (
                    "https://updates.test/latest".into(),
                    MAX_METADATA_BYTES
                ),
                (
                    "https://updates.test/download/v1.1.0/SHA256SUMS".into(),
                    MAX_METADATA_BYTES
                ),
                (
                    "https://updates.test/download/v1.1.0/aru-1.1.0-x86_64-unknown-linux-musl.tar.gz".into(),
                    MAX_ARCHIVE_BYTES
                ),
            ]
        );
    }

    #[test]
    fn dry_run_validates_the_complete_archive_without_replacing() {
        let temporary = tempfile::tempdir().unwrap();
        let context = context(&temporary, true);
        let archive = tar_gz(&[("aru", b"new binary", EntryType::Regular)]);
        let fetcher = updater(&context, archive, None);
        let replacer = FakeReplacer::new(context.executable.clone());

        let outcome = execute_with(context, &fetcher, &replacer).unwrap();

        assert_eq!(outcome.action, UpdateAction::WouldUpdate);
        assert_eq!(replacer.calls(), 0);
        assert_eq!(std::fs::read(outcome.executable).unwrap(), b"old binary");
        assert_eq!(fetcher.requests.lock().unwrap().len(), 3);
    }

    #[test]
    fn equal_and_newer_local_versions_do_not_download_an_archive() {
        for (current, action) in [
            ("1.1.0", UpdateAction::UpToDate),
            ("1.2.0", UpdateAction::LocalNewer),
        ] {
            let temporary = tempfile::tempdir().unwrap();
            let mut context = context(&temporary, false);
            context.current_version = Version::parse(current).unwrap();
            let mut fetcher = FakeFetcher::default();
            fetcher.responses.insert(
                context.source.latest_url.clone(),
                br#"{"tag_name":"v1.1.0"}"#.to_vec(),
            );
            let replacer = FakeReplacer::new(context.executable.clone());

            let outcome = execute_with(context, &fetcher, &replacer).unwrap();

            assert_eq!(outcome.action, action);
            assert_eq!(replacer.calls(), 0);
            assert_eq!(fetcher.requests.lock().unwrap().len(), 1);
            assert_eq!(std::fs::read(outcome.executable).unwrap(), b"old binary");
        }
    }

    #[test]
    fn unstable_or_malformed_release_tags_fail_before_archive_download() {
        for tag in ["1.1.0", "v1.1", "v1.1.0-beta.1", "v01.1.0"] {
            let temporary = tempfile::tempdir().unwrap();
            let context = context(&temporary, false);
            let mut fetcher = FakeFetcher::default();
            fetcher.responses.insert(
                context.source.latest_url.clone(),
                format!(r#"{{"tag_name":"{tag}"}}"#).into_bytes(),
            );
            let replacer = FakeReplacer::new(context.executable.clone());

            let error = execute_with(context, &fetcher, &replacer).unwrap_err();

            assert!(error.to_string().contains("must match vX.Y.Z"), "{error}");
            assert_eq!(replacer.calls(), 0);
            assert_eq!(fetcher.requests.lock().unwrap().len(), 1);
        }
    }

    #[test]
    fn checksum_and_archive_failures_preserve_the_binary() {
        let cases = [
            (
                tar_gz(&[("aru", b"new binary", EntryType::Regular)]),
                Some("0000000000000000000000000000000000000000000000000000000000000000"),
                "checksum verification failed",
            ),
            (
                tar_gz(&[
                    ("aru", b"new binary", EntryType::Regular),
                    ("extra", b"unexpected", EntryType::Regular),
                ]),
                None,
                "unexpected contents",
            ),
            (
                tar_gz(&[("aru", b"link target", EntryType::Symlink)]),
                None,
                "unexpected contents",
            ),
        ];

        for (archive, checksum_override, message) in cases {
            let temporary = tempfile::tempdir().unwrap();
            let context = context(&temporary, false);
            let fetcher = updater(&context, archive, checksum_override);
            let replacer = FakeReplacer::new(context.executable.clone());

            let error = execute_with(context, &fetcher, &replacer).unwrap_err();

            assert!(error.to_string().contains(message), "{error}");
            assert_eq!(replacer.calls(), 0);
            assert_eq!(std::fs::read(&replacer.destination).unwrap(), b"old binary");
        }
    }

    #[test]
    fn windows_zip_is_validated_before_replacement() {
        let temporary = tempfile::tempdir().unwrap();
        let mut context = context(&temporary, false);
        context.target = ReleaseTarget {
            triple: "x86_64-pc-windows-msvc",
            binary_name: "aru.exe",
            archive_kind: ArchiveKind::Zip,
        };
        let archive = zip(&[("aru.exe", b"new windows binary")]);
        let fetcher = updater(&context, archive, None);
        let replacer = FakeReplacer::new(context.executable.clone());

        let outcome = execute_with(context, &fetcher, &replacer).unwrap();

        assert_eq!(outcome.action, UpdateAction::Updated);
        assert_eq!(
            std::fs::read(outcome.executable).unwrap(),
            b"new windows binary"
        );
    }

    #[test]
    fn policy_network_and_replacement_failures_preserve_the_binary() {
        let temporary = tempfile::tempdir().unwrap();
        let mut non_standalone_context = context(&temporary, false);
        let fetcher = FakeFetcher::default();
        let replacer = FakeReplacer::new(non_standalone_context.executable.clone());

        non_standalone_context.standalone = false;
        let error = execute_with(non_standalone_context, &fetcher, &replacer).unwrap_err();
        assert!(error.to_string().contains("standalone aru installations"));
        assert!(fetcher.requests.lock().unwrap().is_empty());

        let mut offline_context = context(&temporary, false);
        offline_context.offline = true;
        let error = execute_with(offline_context, &fetcher, &replacer).unwrap_err();
        assert!(error.to_string().contains("offline"));
        assert!(fetcher.requests.lock().unwrap().is_empty());

        let network_context = context(&temporary, false);
        let error = execute_with(network_context, &fetcher, &replacer).unwrap_err();
        assert!(error.to_string().contains("test fetch failed"));
        assert_eq!(std::fs::read(&replacer.destination).unwrap(), b"old binary");

        let replacement_context = context(&temporary, false);
        let archive = tar_gz(&[("aru", b"new binary", EntryType::Regular)]);
        let fetcher = updater(&replacement_context, archive, None);
        let replacer = FakeReplacer::failing(replacement_context.executable.clone());
        let error = execute_with(replacement_context, &fetcher, &replacer).unwrap_err();
        assert!(error.to_string().contains("permission denied"));
        assert_eq!(std::fs::read(&replacer.destination).unwrap(), b"old binary");
    }
}
