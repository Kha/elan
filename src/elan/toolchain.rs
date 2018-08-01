use errors::*;
use notifications::*;
use elan_dist;
use elan_dist::download::DownloadCfg;
use elan_utils::utils;
use elan_dist::dist::{ToolchainDesc};
use elan_dist::manifest::Component;
use config::Cfg;
use env_var;
use install::{self, InstallMethod};
use telemetry;
use telemetry::{Telemetry, TelemetryEvent};

use std::env::consts::EXE_SUFFIX;
use std::ffi::OsString;
use std::process::Command;
use std::path::{Path, PathBuf};
use std::ffi::OsStr;
use std::env;

use url::Url;

use regex::Regex;

/// A fully resolved reference to a toolchain which may or may not exist
pub struct Toolchain<'a> {
    cfg: &'a Cfg,
    name: String,
    raw_name: String,
    path: PathBuf,
    telemetry: telemetry::Telemetry,
    dist_handler: Box<Fn(elan_dist::Notification) + 'a>,
}

/// Used by the `list_component` function
pub struct ComponentStatus {
    pub component: Component,
    pub required: bool,
    pub installed: bool,
    pub available: bool,
}

pub enum UpdateStatus {
    Installed,
    Updated,
    Unchanged,
}

impl<'a> Toolchain<'a> {
    pub fn from(cfg: &'a Cfg, name: &str) -> Result<Self> {
        //We need to replace ":" and "/" with "-" in the toolchain name in order to make a name which is a valid
        //name for a directory.
        let re = Regex::new(r"[:/]").unwrap();
        let sane_name = re.replace_all(name, "-");

        let path = cfg.toolchains_dir.join(&sane_name[..]);

        Ok(Toolchain {
            cfg: cfg,
            name: sane_name.to_string(),
            raw_name: name.to_owned(),
            path: path.clone(),
            telemetry: Telemetry::new(cfg.elan_dir.join("telemetry")),
            dist_handler: Box::new(move |n| {
                (cfg.notify_handler)(n.into())
            })
        })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn desc(&self) -> Result<ToolchainDesc> {
        Ok(try!(ToolchainDesc::from_str(&self.raw_name)))
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    fn is_symlink(&self) -> bool {
        use std::fs;
        fs::symlink_metadata(&self.path).map(|m| m.file_type().is_symlink()).unwrap_or(false)
    }
    pub fn exists(&self) -> bool {
        // HACK: linked toolchains are symlinks, and, contrary to what std docs
        // lead me to believe `fs::metadata`, used by `is_directory` does not
        // seem to follow symlinks on windows.
        let is_symlink = if cfg!(windows) {
            self.is_symlink()
        } else {
            false
        };
        utils::is_directory(&self.path) || is_symlink
    }
    pub fn verify(&self) -> Result<()> {
        Ok(try!(utils::assert_is_directory(&self.path)))
    }
    pub fn remove(&self) -> Result<()> {
        if self.exists() || self.is_symlink() {
            (self.cfg.notify_handler)(Notification::UninstallingToolchain(&self.name));
        } else {
            (self.cfg.notify_handler)(Notification::ToolchainNotInstalled(&self.name));
            return Ok(());
        }
        if let Some(update_hash) = try!(self.update_hash()) {
            try!(utils::remove_file("update hash", &update_hash));
        }
        let result = install::uninstall(&self.path,
                                        &|n| (self.cfg.notify_handler)(n.into()));
        if !self.exists() {
            (self.cfg.notify_handler)(Notification::UninstalledToolchain(&self.name));
        }
        Ok(try!(result))
    }
    fn install(&self, install_method: InstallMethod) -> Result<UpdateStatus> {
        assert!(self.is_valid_install_method(install_method));
        let exists = self.exists();
        if exists {
            (self.cfg.notify_handler)(Notification::UpdatingToolchain(&self.name));
        } else {
            (self.cfg.notify_handler)(Notification::InstallingToolchain(&self.name));
        }
        (self.cfg.notify_handler)
            (Notification::ToolchainDirectory(&self.path, &self.name));
        let updated = try!(install_method.run(&self.path,
                                              &|n| (self.cfg.notify_handler)(n.into())));

        if !updated {
            (self.cfg.notify_handler)(Notification::UpdateHashMatches);
        } else {
            (self.cfg.notify_handler)(Notification::InstalledToolchain(&self.name));
        }

        let status = match (updated, exists) {
            (true, false) => UpdateStatus::Installed,
            (true, true) => UpdateStatus::Updated,
            (false, true) => UpdateStatus::Unchanged,
            (false, false) => UpdateStatus::Unchanged,
        };

        Ok(status)
    }
    fn install_if_not_installed(&self, install_method: InstallMethod) -> Result<UpdateStatus> {
        assert!(self.is_valid_install_method(install_method));
        (self.cfg.notify_handler)(Notification::LookingForToolchain(&self.name));
        if !self.exists() {
            Ok(try!(self.install(install_method)))
        } else {
            (self.cfg.notify_handler)(Notification::UsingExistingToolchain(&self.name));
            Ok(UpdateStatus::Unchanged)
        }
    }
    fn is_valid_install_method(&self, install_method: InstallMethod) -> bool {
        match install_method {
            InstallMethod::Copy(_) |
            InstallMethod::Link(_) |
            InstallMethod::Installer(..) => self.is_custom(),
            InstallMethod::Dist(..) => !self.is_custom(),
        }
    }
    fn update_hash(&self) -> Result<Option<PathBuf>> {
        if self.is_custom() {
            Ok(None)
        } else {
            Ok(Some(try!(self.cfg.get_hash_file(&self.name, true))))
        }
    }

    fn download_cfg(&self) -> DownloadCfg {
        DownloadCfg {
            temp_cfg: &self.cfg.temp_cfg,
            download_dir: &self.cfg.download_dir,
            notify_handler: &*self.dist_handler,
        }
    }

    pub fn install_from_dist(&self, force_update: bool) -> Result<UpdateStatus> {
        if try!(self.cfg.telemetry_enabled()) {
            return self.install_from_dist_with_telemetry(force_update);
        }
        self.install_from_dist_inner(force_update)
    }

    pub fn install_from_dist_inner(&self, force_update: bool) -> Result<UpdateStatus> {
        let update_hash = try!(self.update_hash());
        self.install(InstallMethod::Dist(&try!(self.desc()),
                                         update_hash.as_ref().map(|p| &**p),
                                         self.download_cfg(),
                                         force_update))
    }

    pub fn install_from_dist_with_telemetry(&self, force_update: bool) -> Result<UpdateStatus> {
        let result = self.install_from_dist_inner(force_update);

        match result {
            Ok(us) => {
                let te = TelemetryEvent::ToolchainUpdate { toolchain: self.name().to_string() ,
                                                           success: true };
                match self.telemetry.log_telemetry(te) {
                    Ok(_) => Ok(us),
                    Err(e) => {
                        (self.cfg.notify_handler)(Notification::TelemetryCleanupError(&e));
                        Ok(us)
                    }
                }
            }
            Err(e) => {
                let te = TelemetryEvent::ToolchainUpdate { toolchain: self.name().to_string() ,
                                                           success: true };
                let _ = self.telemetry.log_telemetry(te).map_err(|xe| {
                    (self.cfg.notify_handler)(Notification::TelemetryCleanupError(&xe));
                });
                Err(e)
            }
        }
    }

    pub fn install_from_dist_if_not_installed(&self) -> Result<UpdateStatus> {
        let update_hash = try!(self.update_hash());
        self.install_if_not_installed(InstallMethod::Dist(&try!(self.desc()),
                                                          update_hash.as_ref().map(|p| &**p),
                                                          self.download_cfg(),
                                                          false))
    }
    pub fn is_custom(&self) -> bool {
        ToolchainDesc::from_str(&self.raw_name).is_err()
    }
    pub fn is_tracking(&self) -> bool {
        ToolchainDesc::from_str(&self.raw_name).ok().map(|d| d.is_tracking()) == Some(true)
    }

    fn ensure_custom(&self) -> Result<()> {
        if !self.is_custom() {
            Err(ErrorKind::Dist(::elan_dist::ErrorKind::InvalidCustomToolchainName(self.name.to_string())).into())
        } else {
            Ok(())
        }
    }

    pub fn install_from_installers(&self, installers: &[&OsStr]) -> Result<()> {
        try!(self.ensure_custom());

        try!(self.remove());

        // FIXME: This should do all downloads first, then do
        // installs, and do it all in a single transaction.
        for installer in installers {
            let installer_str = installer.to_str().unwrap_or("bogus");
            match installer_str.rfind('.') {
                Some(i) => {
                    let extension = &installer_str[i+1..];
                    if extension != "gz" {
                        return Err(ErrorKind::BadInstallerType(extension.to_string()).into());
                    }
                }
                None => return Err(ErrorKind::BadInstallerType(String::from("(none)")).into())
            }

            // FIXME: Pretty hacky
            let is_url = installer_str.starts_with("file://")
                || installer_str.starts_with("http://")
                || installer_str.starts_with("https://");
            let url = Url::parse(installer_str).ok();
            let url = if is_url { url } else { None };
            if let Some(url) = url {

                // Download to a local file
                let local_installer = try!(self.cfg.temp_cfg.new_file_with_ext("", ".tar.gz"));
                try!(utils::download_file(&url,
                                          &local_installer,
                                          None,
                                          &|n| (self.cfg.notify_handler)(n.into())));
                try!(self.install(InstallMethod::Installer(&local_installer, &self.cfg.temp_cfg)));
            } else {
                // If installer is a filename

                // No need to download
                let local_installer = Path::new(installer);

                // Install from file
                try!(self.install(InstallMethod::Installer(&local_installer, &self.cfg.temp_cfg)));
            }
        }

        Ok(())
    }

    pub fn install_from_dir(&self, src: &Path, link: bool) -> Result<()> {
        try!(self.ensure_custom());

        let mut pathbuf = PathBuf::from(src);

        pathbuf.push("bin");
        try!(utils::assert_is_directory(&pathbuf));
        pathbuf.push(format!("lean{}", EXE_SUFFIX));
        try!(utils::assert_is_file(&pathbuf));

        if link {
            try!(self.install(InstallMethod::Link(&try!(utils::to_absolute(src)))));
        } else {
            try!(self.install(InstallMethod::Copy(src)));
        }

        Ok(())
    }

    pub fn create_command<T: AsRef<OsStr>>(&self, binary: T) -> Result<Command> {
        if !self.exists() {
            return Err(ErrorKind::ToolchainNotInstalled(self.name.to_owned()).into());
        }

        // Create the path to this binary within the current toolchain sysroot
        let binary = if let Some(binary_str) = binary.as_ref().to_str() {
            if binary_str.to_lowercase().ends_with(EXE_SUFFIX) {
                binary.as_ref().to_owned()
            } else {
                OsString::from(format!("{}{}", binary_str, EXE_SUFFIX))
            }
        } else {
            // Very weird case. Non-unicode command.
            binary.as_ref().to_owned()
        };

        let bin_path = self.path.join("bin").join(&binary);
        let path = if utils::is_file(&bin_path) {
            &bin_path
        } else {
            let recursion_count = env::var("LEAN_RECURSION_COUNT").ok()
                .and_then(|s| s.parse().ok()).unwrap_or(0);
            if recursion_count > env_var::LEAN_RECURSION_COUNT_MAX - 1 {
                return Err(ErrorKind::BinaryNotFound(self.name.clone(),
                                                     binary.to_string_lossy()
                                                           .into())
                            .into())
            }
            Path::new(&binary)
        };
        let mut cmd = Command::new(&path);
        self.set_env(&mut cmd);
        Ok(cmd)
    }

    // Create a command as a fallback for another toolchain. This is used
    // to give custom toolchains access to leanpkg
    pub fn create_fallback_command<T: AsRef<OsStr>>(&self, binary: T,
                                                    primary_toolchain: &Toolchain) -> Result<Command> {
        // With the hacks below this only works for leanpkg atm
        assert!(binary.as_ref() == "leanpkg" || binary.as_ref() == "leanpkg.exe");

        if !self.exists() {
            return Err(ErrorKind::ToolchainNotInstalled(self.name.to_owned()).into());
        }
        if !primary_toolchain.exists() {
            return Err(ErrorKind::ToolchainNotInstalled(primary_toolchain.name.to_owned()).into());
        }

        let src_file = self.path.join("bin").join(format!("leanpkg{}", EXE_SUFFIX));

        // MAJOR HACKS: Copy leanpkg.exe to its own directory on windows before
        // running it. This is so that the fallback leanpkg, when it in turn runs
        // lean.exe, will run the lean.exe out of the PATH environment
        // variable, _not_ the lean.exe sitting in the same directory as the
        // fallback. See the `fallback_leanpkg_calls_correct_lean` testcase and
        // PR 812.
        //
        // On Windows, spawning a process will search the running application's
        // directory for the exe to spawn before searching PATH, and we don't want
        // it to do that, because leanpkg's directory contains the _wrong_ lean. See
        // the documantation for the lpCommandLine argument of CreateProcess.
        let exe_path = if cfg!(windows) {
            use std::fs;
            let fallback_dir = self.cfg.elan_dir.join("fallback");
            try!(fs::create_dir_all(&fallback_dir)
                 .chain_err(|| "unable to create dir to hold fallback exe"));
            let fallback_file = fallback_dir.join("leanpkg.exe");
            if fallback_file.exists() {
                try!(fs::remove_file(&fallback_file)
                     .chain_err(|| "unable to unlink old fallback exe"));
            }
            try!(fs::hard_link(&src_file, &fallback_file)
                 .chain_err(|| "unable to hard link fallback exe"));
            fallback_file
        } else {
            src_file
        };
        let mut cmd = Command::new(exe_path);
        self.set_env(&mut cmd);
        cmd.env("ELAN_TOOLCHAIN", &primary_toolchain.name);
        Ok(cmd)
    }

    fn set_env(&self, cmd: &mut Command) {
        self.set_ldpath(cmd);

        // Because elan and leanpkg use slightly different
        // definitions of leanpkg home (elan doesn't read HOME on
        // windows), we must set it here to ensure leanpkg and
        // elan agree.
        if let Ok(elan_home) = utils::elan_home() {
            cmd.env("ELAN_HOME", &elan_home);
        }

        env_var::inc("LEAN_RECURSION_COUNT", cmd);

        cmd.env("ELAN_TOOLCHAIN", &self.name);
        cmd.env("ELAN_HOME", &self.cfg.elan_dir);
    }

    pub fn set_ldpath(&self, cmd: &mut Command) {
        let new_path = self.path.join("lib");

        #[cfg(not(target_os = "macos"))]
        mod sysenv {
            pub const LOADER_PATH: &'static str = "LD_LIBRARY_PATH";
        }
        #[cfg(target_os = "macos")]
        mod sysenv {
            pub const LOADER_PATH: &'static str = "DYLD_LIBRARY_PATH";
        }
        env_var::prepend_path(sysenv::LOADER_PATH, vec![new_path.clone()], cmd);

        // Prepend ELAN_HOME/bin to the PATH variable so that we're sure to run
        // leanpkg/lean via the proxy bins. There is no fallback case for if the
        // proxy bins don't exist. We'll just be running whatever happens to
        // be on the PATH.
        let mut path_entries = vec![];
        if let Ok(elan_home) = utils::elan_home() {
            path_entries.push(elan_home.join("bin").to_path_buf());
        }

        if cfg!(target_os = "windows") {
            path_entries.push(self.path.join("bin"));
        }

        env_var::prepend_path("PATH", path_entries, cmd);
    }

    pub fn doc_path(&self, relative: &str) -> Result<PathBuf> {
        try!(self.verify());

        let parts = vec!["share", "doc", "lean", "html"];
        let mut doc_dir = self.path.clone();
        for part in parts {
            doc_dir.push(part);
        }
        doc_dir.push(relative);

        Ok(doc_dir)
    }
    pub fn open_docs(&self, relative: &str) -> Result<()> {
        try!(self.verify());

        Ok(try!(utils::open_browser(&try!(self.doc_path(relative)))))
    }

    pub fn make_default(&self) -> Result<()> {
        self.cfg.set_default(&self.name)
    }
    pub fn make_override(&self, path: &Path) -> Result<()> {
        Ok(try!(self.cfg.settings_file.with_mut(|s| {
            s.add_override(path, self.name.clone(), self.cfg.notify_handler.as_ref());
            Ok(())
        })))
    }

    pub fn binary_file(&self, name: &str) -> PathBuf {
        let mut path = self.path.clone();
        path.push("bin");
        path.push(name.to_owned() + env::consts::EXE_SUFFIX);
        path
    }
}
