#[cfg(target_os = "macos")]
use std::process::Command;

const SERVICE: &str = "dev.packager.package";

fn account(package_id: &str, key: &str) -> String {
    format!("{package_id}:{key}")
}

#[cfg(target_os = "macos")]
pub fn set(package_id: &str, key: &str, value: &str) -> Result<(), String> {
    let output = Command::new("/usr/bin/security")
        .args([
            "add-generic-password",
            "-U",
            "-s",
            SERVICE,
            "-a",
            &account(package_id, key),
            "-w",
            value,
        ])
        .output()
        .map_err(|error| format!("Cannot access macOS Keychain: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Cannot save {key} in macOS Keychain: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(target_os = "macos")]
pub fn get(package_id: &str, key: &str) -> Result<String, String> {
    let output = Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-s",
            SERVICE,
            "-a",
            &account(package_id, key),
            "-w",
        ])
        .output()
        .map_err(|error| format!("Cannot access macOS Keychain: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(format!("{key} is missing from macOS Keychain"))
    }
}

#[cfg(target_os = "macos")]
pub fn remove(package_id: &str, key: &str) -> Result<(), String> {
    let output = Command::new("/usr/bin/security")
        .args([
            "delete-generic-password",
            "-s",
            SERVICE,
            "-a",
            &account(package_id, key),
        ])
        .output()
        .map_err(|error| format!("Cannot access macOS Keychain: {error}"))?;
    if output.status.success() || output.status.code() == Some(44) {
        Ok(())
    } else {
        Err(format!(
            "Cannot remove {key} from macOS Keychain: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(target_os = "windows")]
fn entry(package_id: &str, key: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, &account(package_id, key))
        .map_err(|error| format!("Cannot access Windows Credential Manager: {error}"))
}

#[cfg(target_os = "windows")]
pub fn set(package_id: &str, key: &str, value: &str) -> Result<(), String> {
    entry(package_id, key)?
        .set_password(value)
        .map_err(|error| format!("Cannot save {key} in Windows Credential Manager: {error}"))
}

#[cfg(target_os = "windows")]
pub fn get(package_id: &str, key: &str) -> Result<String, String> {
    entry(package_id, key)?
        .get_password()
        .map_err(|_| format!("{key} is missing from Windows Credential Manager"))
}

#[cfg(target_os = "windows")]
pub fn remove(package_id: &str, key: &str) -> Result<(), String> {
    match entry(package_id, key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!(
            "Cannot remove {key} from Windows Credential Manager: {error}"
        )),
    }
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub fn set(_package_id: &str, _key: &str, _value: &str) -> Result<(), String> {
    Err("Secure package secrets are supported on macOS and Windows".into())
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub fn get(_package_id: &str, key: &str) -> Result<String, String> {
    Err(format!("{key} is unavailable on this platform"))
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub fn remove(_package_id: &str, _key: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keychain_account_is_namespaced() {
        assert_eq!(account("open-notebook", "TOKEN"), "open-notebook:TOKEN");
    }
}
