use download::DownloadCfg;
use elan_utils::utils::fetch_url;
use elan_utils::{self, utils};
use errors::*;
use manifest::Component;
use manifestation::Manifestation;
use notifications::Notification;
use prefix::InstallPrefix;
use temp;

use std::{fmt, env};
use std::path::Path;

use regex::Regex;

const DEFAULT_ORIGIN: &str = "leanprover/lean4";
// index file used as `ELAN_INDEX_URL` default for `DEFAULT_ORIGIN` requests
const DEFAULT_INDEX_URL: &str = "https://release.lean-lang.org/index.json";

// Fully-resolved toolchain descriptors. These always have full target
// triples attached to them and are used for canonical identification,
// such as naming their installation directory.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolchainDesc {
    // The GitHub source repository to use (if "nightly" is specified, we append "-nightly" to this)
    // If None, we default to "leanprover/lean"
    pub origin: Option<String>,
    // Either "nightly", "beta", "stable", an explicit version number, or a tag name
    pub channel: String,
    pub date: Option<String>,
}

impl ToolchainDesc {
    pub fn from_str(name: &str) -> Result<Self> {
        let pattern = r"^(?:([a-zA-Z0-9-]+[/][a-zA-Z0-9-]+)[:])?(?:(nightly|beta|stable)(?:-(\d{4}-\d{2}-\d{2}))?|([a-zA-Z0-9-.]+))$";

        let re = Regex::new(&pattern).unwrap();
        if let Some(c) = re.captures(name) {
            fn fn_map(s: &str) -> Option<String> {
                if s == "" {
                    None
                } else {
                    Some(s.to_owned())
                }
            }
            let origin = c.get(1).map(|s| s.as_str()).and_then(fn_map);
            let tag = c.get(4).map(|m| m.as_str());
            if let (Some(ref origin), Some("lean-toolchain")) = (&origin, tag) {
                let toolchain_url = format!("https://raw.githubusercontent.com/{}/HEAD/lean-toolchain", origin);
                return ToolchainDesc::from_str(fetch_url(&toolchain_url)?.trim())
            }

            Ok(ToolchainDesc {
                origin,
                channel: c.get(2).map(|s| s.as_str().to_owned()).or(tag.map(|t| t.to_owned())).unwrap(),
                date: c.get(3).map(|s| s.as_str()).and_then(fn_map),
            })
        } else {
            Err(ErrorKind::InvalidToolchainName(name.to_string()).into())
        }
    }

    /// Either "$channel" or "channel-$date"
    pub fn manifest_name(&self) -> String {
        match self.date {
            None => self.channel.clone(),
            Some(ref date) => format!("{}-{}", self.channel, date),
        }
    }

    pub fn package_dir(&self, dist_root: &str) -> String {
        match self.date {
            None => format!("{}", dist_root),
            Some(ref date) => format!("{}/{}", dist_root, date),
        }
    }

    pub fn full_spec(&self) -> String {
        if self.date.is_some() {
            format!("{}", self)
        } else {
            format!("{} (tracking)", self)
        }
    }

    pub fn is_tracking(&self) -> bool {
        let channels = ["nightly", "beta", "stable"];
        channels.iter().any(|x| *x == self.channel) && self.date.is_none()
    }
}

#[derive(Debug)]
pub struct Manifest<'a>(temp::File<'a>, String);

impl fmt::Display for ToolchainDesc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(ref origin) = self.origin {
            write!(f, "{}:", origin)?;
        }

        write!(f, "{}", &self.channel)?;

        if let Some(ref date) = self.date {
            write!(f, "-{}", date)?;
        }

        Ok(())
    }
}

// Installs or updates a toolchain from a dist server. If an initial
// install then it will be installed with the default components. If
// an upgrade then all the existing components will be upgraded.
//
// Returns the manifest's hash if anything changed.
pub fn update_from_dist<'a>(
    download: DownloadCfg<'a>,
    update_hash: Option<&Path>,
    toolchain: &ToolchainDesc,
    prefix: &InstallPrefix,
    add: &[Component],
    remove: &[Component],
    force_update: bool,
) -> Result<Option<String>> {
    let fresh_install = !prefix.path().exists();

    let res = update_from_dist_(
        download,
        update_hash,
        toolchain,
        prefix,
        add,
        remove,
        force_update,
    );

    // Don't leave behind an empty / broken installation directory
    if res.is_err() && fresh_install {
        // FIXME Ignoring cascading errors
        let _ = utils::remove_dir("toolchain", prefix.path(), &|n| {
            (download.notify_handler)(n.into())
        });
    }

    res
}

//Append "-nightly" to the origin if version == "nightly" was specified.
//If origin is None use DEFAULT_ORIGIN.
fn build_origin_name(origin: Option<&String>, version: &str) -> String {
    let repo = match origin {
        None => DEFAULT_ORIGIN,
        Some(repo) => repo,
    };
    format!(
        "{}{}",
        repo,
        if version == "nightly" { "-nightly" } else { "" }
    )
}

pub fn update_from_dist_<'a>(
    download: DownloadCfg<'a>,
    update_hash: Option<&Path>,
    toolchain: &ToolchainDesc,
    prefix: &InstallPrefix,
    _add: &[Component],
    _remove: &[Component],
    _force_update: bool,
) -> Result<Option<String>> {
    let toolchain_str = toolchain.to_string();
    let manifestation = Manifestation::open(prefix.clone())?;

    let url = match toolchain_url(download, toolchain) {
        Ok(url) => url,
        Err(Error(ErrorKind::Utils(elan_utils::ErrorKind::DownloadNotExists { .. }), _)) => {
            return Err(format!("no release found for '{}'", toolchain.manifest_name()).into());
        }
        Err(e @ Error(ErrorKind::ChecksumFailed { .. }, _)) => {
            return Err(e);
        }
        Err(e) => {
            return Err(e).chain_err(|| {
                format!(
                    "failed to resolve latest version of '{}'",
                    toolchain.manifest_name()
                )
            });
        }
    };

    if let Some(hash_file) = update_hash {
        if utils::is_file(hash_file) {
            if let Ok(contents) = utils::read_file("update hash", hash_file) {
                if contents == url {
                    // Skip download, url matches
                    return Ok(None);
                }
            } /*else {
                  (self.notify_handler)(Notification::CantReadUpdateHash(hash_file));
              }*/
        } /*else {
              (self.notify_handler)(Notification::NoUpdateHash(hash_file));
          }*/
    }

    match manifestation.update(
        &build_origin_name(toolchain.origin.as_ref(), &toolchain.channel),
        &url,
        &download.temp_cfg,
        download.notify_handler.clone(),
    ) {
        Ok(()) => Ok(()),
        e @ Err(Error(ErrorKind::Utils(elan_utils::ErrorKind::DownloadNotExists { .. }), _)) => e
            .chain_err(|| {
                format!(
                    "could not download nonexistent lean version `{}`",
                    toolchain_str
                )
            }),
        Err(e) => Err(e),
    }
    .map(|()| Some(url))
}

pub enum ToolchainResolution {
    Index(json::JsonValue),
    Html(String),
}

fn toolchain_url<'a>(download: DownloadCfg<'a>, toolchain: &ToolchainDesc) -> Result<ToolchainResolution> {
    if toolchain.origin.iter().all(|o| o == DEFAULT_ORIGIN) {
        let index = fetch_url(env::var("ELAN_INDEX_URL").map(|i| i.as_str()).unwrap_or(DEFAULT_INDEX_URL))?;
        let json = json::parse(&index).expect("failed to parse index");
        if toolchain.is_tracking() {
           return Ok(ToolchainResolution::Index(json[toolchain.channel][0]["assets"]))
        }
        if let Some(release) = json.entries().flat_map(|chan| chan.1.members()).find(|release| release["name"] == toolchain.channel) {
            return Ok(ToolchainResolution::Index(release.clone()))
        }
    }
    let origin = build_origin_name(toolchain.origin.as_ref(), toolchain.channel.as_ref());
    Ok(ToolchainResolution::Html(
        match (toolchain.date.as_ref(), toolchain.channel.as_str()) {
            (None, version) if version == "stable" || version == "nightly" => {
                (download.notify_handler)(Notification::DownloadingManifest(version));
                let release = utils::fetch_latest_release_tag(&origin)?;
                (download.notify_handler)(Notification::DownloadedManifest(
                    version,
                    Some(&release),
                ));
                format!("https://github.com/{}/releases/expanded_assets/{}", origin, release)
            }
            (Some(date), "nightly") => format!(
                "https://github.com/{}/releases/expanded_assets/nightly-{}",
                origin, date
            ),
            (None, version) if version.starts_with(|c: char| c.is_numeric()) => {
                format!("https://github.com/{}/releases/expanded_assets/v{}", origin, version)
            }
            (None, tag) => format!("https://github.com/{}/releases/expanded_assets/{}", origin, tag),
            _ => panic!("wat"),
        },
    ))
}

pub fn host_triple() -> &'static str {
    include_str!(concat!(env!("OUT_DIR"), "/target.txt"))
}
