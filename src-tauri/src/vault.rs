//! 凭据保险箱：密码 / 私钥口令存入 Windows 凭据管理器，数据库不落明文。
const SERVICE: &str = "DeployPilot";

fn entry(server_id: i64) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, &format!("server-{server_id}")).map_err(|e| e.to_string())
}

pub fn set_secret(server_id: i64, secret: &str) -> Result<(), String> {
    entry(server_id)?
        .set_password(secret)
        .map_err(|e| format!("写入凭据失败: {e}"))
}

pub fn get_secret(server_id: i64) -> Result<Option<String>, String> {
    match entry(server_id)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("读取凭据失败: {e}")),
    }
}

pub fn delete_secret(server_id: i64) -> Result<(), String> {
    match entry(server_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("删除凭据失败: {e}")),
    }
}