//! The `fileboat` command for terminals: `fileboat .`, `fileboat D:\x`, `fileboat notes.md`.
//!
//! Fileboat already opens a path named on its command line (see `launch`); these commands put a
//! small launcher on the PATH that starts it without holding the terminal. On Windows the
//! launchers (`fileboat.cmd` for cmd and PowerShell, an extensionless `fileboat` for Git Bash) go
//! in `%LOCALAPPDATA%\Fileboat\bin`, which is added to the user's PATH; on macOS one script goes
//! in `/usr/local/bin`, asking for an administrator password when that folder needs one.

/// In every launcher, so removing one never deletes a `fileboat` someone else put there.
const MARK: &str = "Fileboat command line launcher";

/// Install the launcher for this copy of Fileboat. Resolves to a sentence for the user.
#[tauri::command]
pub async fn cli_install() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Could not find Fileboat's program file: {e}"))?;
    tauri::async_runtime::spawn_blocking(move || imp::install(&exe)).await.map_err(|e| e.to_string())?
}

/// Remove the launcher (and, on Windows, its folder from the PATH).
#[tauri::command]
pub async fn cli_uninstall() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(imp::uninstall).await.map_err(|e| e.to_string())?
}

/// A path inside single quotes for `sh`.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The launcher for `sh` (Git Bash, macOS): starts Fileboat in the background with the arguments.
/// `exe` is a shell expression for the program.
fn sh_script(exe: &str) -> String {
    format!(
        "#!/bin/sh\n# {MARK}: opens a folder or file in Fileboat (fileboat [path]).\nexe={exe}\n\
         [ -f \"$exe\" ] || {{ echo \"Fileboat is no longer at $exe.\" >&2; exit 1; }}\n\
         nohup \"$exe\" \"$@\" >/dev/null 2>&1 &\n"
    )
}

#[cfg(windows)]
mod imp {
    use super::{MARK, sh_quote, sh_script};
    use std::path::{Path, PathBuf};

    /// `%LOCALAPPDATA%\Fileboat\bin` (beside the installed program), or `bin` in
    /// `FILEBOAT_DATA_DIR` when that is set.
    fn launcher_dir() -> Result<PathBuf, String> {
        match std::env::var_os("FILEBOAT_DATA_DIR").filter(|v| !v.is_empty()) {
            Some(custom) => Ok(PathBuf::from(custom).join("bin")),
            None => Ok(dirs::data_local_dir().ok_or("Could not find the local app data folder.")?.join("Fileboat").join("bin")),
        }
    }

    /// `fileboat.cmd`. When Fileboat is installed one folder up (the usual case) the program is
    /// found from the script's own folder, so a user name with accents can't trip cmd's code page.
    fn cmd_script(exe: &Path, dir: &Path) -> String {
        let program = if exe.parent() == dir.parent() {
            format!("%~dp0..\\{}", exe.file_name().unwrap_or_default().to_string_lossy())
        } else {
            exe.display().to_string().replace('%', "%%")
        };
        format!(
            "@echo off\r\nrem {MARK}: opens a folder or file in Fileboat (fileboat [path]).\r\n\
             if not exist \"{program}\" goto missing\r\nstart \"\" \"{program}\" %*\r\nexit /b 0\r\n\
             :missing\r\necho Fileboat is no longer at \"{program}\". 1>&2\r\nexit /b 1\r\n"
        )
    }

    /// The Git Bash launcher, finding the program the same way.
    fn bash_script(exe: &Path, dir: &Path) -> String {
        let program = if exe.parent() == dir.parent() {
            format!("\"$(dirname \"$0\")\"/../{}", sh_quote(&exe.file_name().unwrap_or_default().to_string_lossy()))
        } else {
            sh_quote(&exe.display().to_string().replace('\\', "/"))
        };
        sh_script(&program)
    }

    pub fn install(exe: &Path) -> Result<String, String> {
        let dir = launcher_dir()?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
        let write = |name: &str, text: String| {
            let path = dir.join(name);
            std::fs::write(&path, text).map_err(|e| format!("Could not write {}: {e}", path.display()))
        };
        write("fileboat.cmd", cmd_script(exe, &dir))?;
        write("fileboat", bash_script(exe, &dir))?;
        let added = user_path::add(user_path::ENVIRONMENT, &dir.display().to_string())?;
        if added {
            user_path::announce();
        }
        Ok("Installed. In a new terminal, `fileboat .` opens the folder you are in.".into())
    }

    pub fn uninstall() -> Result<String, String> {
        let dir = launcher_dir()?;
        for name in ["fileboat.cmd", "fileboat"] {
            let path = dir.join(name);
            let ours = std::fs::read_to_string(&path).is_ok_and(|t| t.contains(MARK));
            if ours {
                std::fs::remove_file(&path).map_err(|e| format!("Could not remove {}: {e}", path.display()))?;
            }
        }
        let _ = std::fs::remove_dir(&dir); // only if nothing else is in it
        if user_path::remove(user_path::ENVIRONMENT, &dir.display().to_string())? {
            user_path::announce();
        }
        Ok("Removed the fileboat command.".into())
    }

    /// The user's PATH in the registry (`HKCU\Environment`), edited in place: its type is kept
    /// (`REG_EXPAND_SZ`, so `%VAR%` entries still expand) and only one entry is added or removed.
    pub mod user_path {
        use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, LPARAM, WPARAM};
        use windows::Win32::System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE, RegCloseKey, RegOpenKeyExW,
            RegQueryValueExW, RegSetValueExW,
        };
        use windows::Win32::UI::WindowsAndMessaging::{HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE};
        use windows::core::{HSTRING, w};

        pub const ENVIRONMENT: &str = "Environment";

        /// The same folder, whatever the case, quotes or trailing slash.
        fn same_dir(a: &str, b: &str) -> bool {
            let norm = |s: &str| s.trim().trim_matches('"').trim_end_matches(['\\', '/']).to_lowercase();
            !norm(a).is_empty() && norm(a) == norm(b)
        }

        /// `path` with `dir` at the end, or None when it is already in it.
        pub fn with(path: &str, dir: &str) -> Option<String> {
            if path.split(';').any(|p| same_dir(p, dir)) {
                return None;
            }
            let path = path.trim_end_matches(';');
            Some(if path.is_empty() { dir.to_string() } else { format!("{path};{dir}") })
        }

        /// `path` without `dir`, or None when it isn't in it. Other entries are kept as they are.
        pub fn without(path: &str, dir: &str) -> Option<String> {
            let parts: Vec<&str> = path.split(';').collect();
            if !parts.iter().any(|p| same_dir(p, dir)) {
                return None;
            }
            Some(parts.into_iter().filter(|p| !same_dir(p, dir)).collect::<Vec<_>>().join(";"))
        }

        struct Key(HKEY);
        impl Drop for Key {
            fn drop(&mut self) {
                unsafe {
                    let _ = RegCloseKey(self.0);
                }
            }
        }

        fn open(subkey: &str) -> Result<Key, String> {
            let mut key = HKEY::default();
            let err = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, &HSTRING::from(subkey), None, KEY_READ | KEY_WRITE, &mut key) };
            if err != ERROR_SUCCESS {
                return Err(format!("Could not open your environment settings (error {}).", err.0));
            }
            Ok(Key(key))
        }

        /// The PATH value and its type (an empty REG_EXPAND_SZ when there is none yet).
        fn read(key: &Key) -> Result<(String, REG_VALUE_TYPE), String> {
            let mut kind = REG_VALUE_TYPE::default();
            let mut size = 0u32;
            let err = unsafe { RegQueryValueExW(key.0, w!("Path"), None, Some(&mut kind), None, Some(&mut size)) };
            if err == ERROR_FILE_NOT_FOUND {
                return Ok((String::new(), REG_EXPAND_SZ));
            }
            if err != ERROR_SUCCESS {
                return Err(format!("Could not read your PATH (error {}).", err.0));
            }
            if kind != REG_SZ && kind != REG_EXPAND_SZ {
                return Err("Your PATH isn't stored as text; Fileboat left it alone.".into());
            }
            let mut buf = vec![0u16; (size as usize).div_ceil(2) + 1];
            let mut bytes = (buf.len() * 2) as u32;
            let err = unsafe { RegQueryValueExW(key.0, w!("Path"), None, Some(&mut kind), Some(buf.as_mut_ptr().cast()), Some(&mut bytes)) };
            if err != ERROR_SUCCESS {
                return Err(format!("Could not read your PATH (error {}).", err.0));
            }
            buf.truncate(bytes as usize / 2);
            while buf.last() == Some(&0) {
                buf.pop();
            }
            Ok((String::from_utf16_lossy(&buf), kind))
        }

        fn write(key: &Key, value: &str, kind: REG_VALUE_TYPE) -> Result<(), String> {
            let data: Vec<u8> = value.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
            let err = unsafe { RegSetValueExW(key.0, w!("Path"), None, kind, Some(&data)) };
            if err != ERROR_SUCCESS {
                return Err(format!("Could not change your PATH (error {}).", err.0));
            }
            Ok(())
        }

        /// Add `dir` to the PATH under `subkey`; false when it was there already.
        pub fn add(subkey: &str, dir: &str) -> Result<bool, String> {
            let key = open(subkey)?;
            let (path, kind) = read(&key)?;
            match with(&path, dir) {
                Some(new) => write(&key, &new, kind).map(|_| true),
                None => Ok(false),
            }
        }

        /// Take `dir` out of the PATH under `subkey`; false when it wasn't in it.
        pub fn remove(subkey: &str, dir: &str) -> Result<bool, String> {
            let key = open(subkey)?;
            let (path, kind) = read(&key)?;
            match without(&path, dir) {
                Some(new) => write(&key, &new, kind).map(|_| true),
                None => Ok(false),
            }
        }

        /// Tell running programs (Explorer, so terminals started from it) that the PATH changed.
        pub fn announce() {
            unsafe {
                SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(w!("Environment").as_ptr() as isize), SMTO_ABORTIFHUNG, 3000, None);
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn adds_once_and_removes_only_that_entry() {
                let dir = r"C:\Users\me\AppData\Local\Fileboat\bin";
                assert_eq!(with("", dir).as_deref(), Some(dir));
                assert_eq!(with(r"C:\a;%USERPROFILE%\b;", dir), Some(format!(r"C:\a;%USERPROFILE%\b;{dir}")));
                assert_eq!(with(&format!(r"C:\a;{}\", dir.to_uppercase()), dir), None);
                assert_eq!(with(&format!(r#"C:\a;"{dir}""#), dir), None);
                assert_eq!(without(&format!(r"C:\a;{dir};%X%\b"), dir).as_deref(), Some(r"C:\a;%X%\b"));
                assert_eq!(without(r"C:\a;;C:\b", dir), None);
                assert_eq!(without(dir, dir).as_deref(), Some(""));
            }

            /// The registry edits, on a scratch key (never the real PATH): type and other entries kept.
            #[test]
            fn edits_the_registry_value_in_place() {
                use windows::Win32::System::Registry::{RegCreateKeyW, RegDeleteTreeW};
                let subkey = format!(r"Software\FileboatTest{}", std::process::id());
                let mut created = HKEY::default();
                assert_eq!(unsafe { RegCreateKeyW(HKEY_CURRENT_USER, &HSTRING::from(&subkey), &mut created) }, ERROR_SUCCESS);
                drop(Key(created));
                let key = open(&subkey).unwrap();
                write(&key, r"%SystemRoot%\x;C:\y", REG_EXPAND_SZ).unwrap();
                assert!(add(&subkey, r"C:\fb\bin").unwrap());
                assert!(!add(&subkey, r"c:\FB\bin\").unwrap());
                assert_eq!(read(&key).unwrap(), (r"%SystemRoot%\x;C:\y;C:\fb\bin".to_string(), REG_EXPAND_SZ));
                assert!(remove(&subkey, r"C:\fb\bin").unwrap());
                assert!(!remove(&subkey, r"C:\fb\bin").unwrap());
                assert_eq!(read(&key).unwrap(), (r"%SystemRoot%\x;C:\y".to_string(), REG_EXPAND_SZ));
                drop(key);
                unsafe {
                    let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(subkey));
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn launchers_find_the_installed_program_beside_them() {
            let dir = Path::new(r"C:\Users\José\AppData\Local\Fileboat\bin");
            let exe = Path::new(r"C:\Users\José\AppData\Local\Fileboat\fileboat.exe");
            let cmd = cmd_script(exe, dir);
            assert!(cmd.contains(r#"start "" "%~dp0..\fileboat.exe" %*"#), "{cmd}");
            assert!(!cmd.contains("José"));
            assert!(bash_script(exe, dir).contains(r#"exe="$(dirname "$0")"/../'fileboat.exe'"#));
            let dev = Path::new(r"D:\src\100%\target\debug\fileboat.exe");
            assert!(cmd_script(dev, dir).contains(r#""D:\src\100%%\target\debug\fileboat.exe""#));
            assert!(bash_script(dev, dir).contains("exe='D:/src/100%/target/debug/fileboat.exe'"));
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{MARK, sh_quote, sh_script};
    use std::path::Path;

    const TARGET: &str = "/usr/local/bin/fileboat";

    /// Run a shell command as administrator (macOS asks for the password).
    fn as_admin(command: &str) -> Result<(), String> {
        let script = format!("do shell script \"{}\" with administrator privileges", command.replace('\\', "\\\\").replace('"', "\\\""));
        let out = std::process::Command::new("osascript").args(["-e", &script]).output().map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!("Could not change {TARGET}: {}", String::from_utf8_lossy(&out.stderr).trim()))
        }
    }

    pub fn install(exe: &Path) -> Result<String, String> {
        let text = sh_script(&sh_quote(&exe.display().to_string()));
        let direct = std::fs::create_dir_all("/usr/local/bin").and_then(|_| std::fs::write(TARGET, &text)).and_then(|_| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(TARGET, std::fs::Permissions::from_mode(0o755))
        });
        if direct.is_err() {
            let temp = std::env::temp_dir().join(format!("fileboat-launcher-{}", std::process::id()));
            std::fs::write(&temp, &text).map_err(|e| e.to_string())?;
            let result = as_admin(&format!(
                "mkdir -p /usr/local/bin && cp {} {TARGET} && chmod 755 {TARGET}",
                sh_quote(&temp.display().to_string())
            ));
            let _ = std::fs::remove_file(&temp);
            result?;
        }
        Ok("Installed. In a new terminal, `fileboat .` opens the folder you are in.".into())
    }

    pub fn uninstall() -> Result<String, String> {
        if !std::fs::read_to_string(TARGET).is_ok_and(|t| t.contains(MARK)) {
            return Ok("The fileboat command isn't installed.".into());
        }
        if std::fs::remove_file(TARGET).is_err() {
            as_admin(&format!("rm -f {TARGET}"))?;
        }
        Ok("Removed the fileboat command.".into())
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use std::path::Path;

    pub fn install(_exe: &Path) -> Result<String, String> {
        Err("The fileboat command can't be installed on this system yet.".into())
    }

    pub fn uninstall() -> Result<String, String> {
        Err("The fileboat command can't be installed on this system yet.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sh_launcher_backgrounds_the_program_with_its_arguments() {
        let s = sh_script(&sh_quote("/Applications/Fileboat.app/Contents/MacOS/it's"));
        assert!(s.starts_with("#!/bin/sh\n"));
        assert!(s.contains(MARK));
        assert!(s.contains(r"exe='/Applications/Fileboat.app/Contents/MacOS/it'\''s'"));
        assert!(s.contains("nohup \"$exe\" \"$@\" >/dev/null 2>&1 &"));
    }
}
