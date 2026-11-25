use crate::model::{DeployProfile, HistoryRecord, ProfileInput, Project, ProjectInput, Server, ServerInput};
use rusqlite::{params, Connection, OptionalExtension};

pub fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS server (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            host TEXT NOT NULL,
            port INTEGER NOT NULL DEFAULT 22,
            username TEXT NOT NULL,
            auth_type TEXT NOT NULL,
            key_path TEXT,
            has_credential INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
        );
        CREATE TABLE IF NOT EXISTS project (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            local_path TEXT NOT NULL,
            build_command TEXT NOT NULL,
            artifact_path TEXT NOT NULL,
            pre_build_command TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
        );
        CREATE TABLE IF NOT EXISTS deploy_profile (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            server_id INTEGER NOT NULL,
            env_name TEXT NOT NULL DEFAULT 'default',
            remote_dir TEXT NOT NULL,
            site_dir TEXT NOT NULL DEFAULT 'current',
            jar_name TEXT NOT NULL DEFAULT 'app.jar',
            restart_command TEXT NOT NULL DEFAULT '',
            health_check_url TEXT NOT NULL DEFAULT '',
            backup_keep INTEGER NOT NULL DEFAULT 3,
            is_prod INTEGER NOT NULL DEFAULT 0,
            UNIQUE(project_id, server_id, env_name)
        );
        CREATE TABLE IF NOT EXISTS deploy_record (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            server_id INTEGER NOT NULL,
            env_name TEXT NOT NULL DEFAULT 'default',
            kind TEXT NOT NULL DEFAULT 'deploy',
            status TEXT NOT NULL,
            started_at TEXT NOT NULL,
            duration_ms INTEGER NOT NULL DEFAULT 0,
            failed_step TEXT,
            message TEXT,
            log_file TEXT,
            backup_ref TEXT,
            remote_dir TEXT
        );
        CREATE TABLE IF NOT EXISTS setting (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )?;
    // M2 旧库迁移：补齐新增列（已存在则忽略）
    for col in [
        "ALTER TABLE deploy_profile ADD COLUMN restart_command TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE deploy_profile ADD COLUMN health_check_url TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE deploy_profile ADD COLUMN backup_keep INTEGER NOT NULL DEFAULT 3",
        "ALTER TABLE deploy_profile ADD COLUMN is_prod INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE deploy_profile ADD COLUMN site_dir TEXT NOT NULL DEFAULT 'current'",
        "ALTER TABLE deploy_profile ADD COLUMN jar_name TEXT NOT NULL DEFAULT 'app.jar'",
        "ALTER TABLE deploy_record ADD COLUMN env_name TEXT NOT NULL DEFAULT 'default'",
        "ALTER TABLE server ADD COLUMN server_type TEXT NOT NULL DEFAULT 'ssh'",
        "ALTER TABLE server ADD COLUMN panel_url TEXT",
        "ALTER TABLE server ADD COLUMN tls_insecure INTEGER NOT NULL DEFAULT 0",
    ] {
        let _ = conn.execute_batch(col);
    }
    // M3 旧库迁移：deploy_profile 增加 env_name 并重建唯一约束
    let has_env = conn.prepare("SELECT env_name FROM deploy_profile LIMIT 0").is_ok();
    if !has_env {
        conn.execute_batch(
            "CREATE TABLE deploy_profile_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER NOT NULL,
                server_id INTEGER NOT NULL,
                env_name TEXT NOT NULL DEFAULT 'default',
                remote_dir TEXT NOT NULL,
                site_dir TEXT NOT NULL DEFAULT 'current',
                jar_name TEXT NOT NULL DEFAULT 'app.jar',
                restart_command TEXT NOT NULL DEFAULT '',
                health_check_url TEXT NOT NULL DEFAULT '',
                backup_keep INTEGER NOT NULL DEFAULT 3,
                is_prod INTEGER NOT NULL DEFAULT 0,
                UNIQUE(project_id, server_id, env_name)
            );
            INSERT INTO deploy_profile_new (project_id, server_id, env_name, remote_dir, site_dir, jar_name, restart_command, health_check_url, backup_keep, is_prod)
                SELECT project_id, server_id, 'default', remote_dir, 'current', 'app.jar', restart_command, health_check_url, backup_keep, is_prod FROM deploy_profile;
            DROP TABLE deploy_profile;
            ALTER TABLE deploy_profile_new RENAME TO deploy_profile;",
        )?;
    }
    Ok(())
}

fn row_to_server(row: &rusqlite::Row) -> rusqlite::Result<Server> {
    Ok(Server {
        id: row.get(0)?,
        name: row.get(1)?,
        host: row.get(2)?,
        port: row.get::<_, i64>(3)? as u16,
        username: row.get(4)?,
        auth_type: row.get(5)?,
        key_path: row.get(6)?,
        has_credential: row.get::<_, i64>(7)? != 0,
        created_at: row.get(8)?,
        server_type: row.get(9)?,
        panel_url: row.get(10)?,
        tls_insecure: row.get::<_, i64>(11)? != 0,
    })
}

const SERVER_COLS: &str =
    "id, name, host, port, username, auth_type, key_path, has_credential, created_at, server_type, panel_url, tls_insecure";

pub fn list_servers(conn: &Connection) -> Result<Vec<Server>, String> {
    let mut stmt = conn
        .prepare(&format!("SELECT {SERVER_COLS} FROM server ORDER BY id"))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_server)
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())
}

pub fn get_server(conn: &Connection, id: i64) -> Result<Server, String> {
    conn.query_row(
        &format!("SELECT {SERVER_COLS} FROM server WHERE id = ?1"),
        params![id],
        row_to_server,
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("服务器不存在 (id={id})"))
}

pub fn insert_server(conn: &Connection, input: &ServerInput) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO server (name, host, port, username, auth_type, key_path, server_type, panel_url, tls_insecure)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![input.name, input.host, input.port as i64, input.username, input.auth_type, input.key_path,
                input.server_type, input.panel_url, input.tls_insecure as i64],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

pub fn update_server(conn: &Connection, id: i64, input: &ServerInput) -> Result<(), String> {
    let n = conn.execute(
        "UPDATE server SET name=?1, host=?2, port=?3, username=?4, auth_type=?5, key_path=?6,
                           server_type=?7, panel_url=?8, tls_insecure=?9 WHERE id=?10",
        params![input.name, input.host, input.port as i64, input.username, input.auth_type, input.key_path,
                input.server_type, input.panel_url, input.tls_insecure as i64, id],
    )
    .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("服务器不存在 (id={id})"));
    }
    Ok(())
}

pub fn set_has_credential(conn: &Connection, id: i64, has: bool) -> Result<(), String> {
    conn.execute(
        "UPDATE server SET has_credential=?1 WHERE id=?2",
        params![has as i64, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_server(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM server WHERE id=?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn row_to_project(row: &rusqlite::Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        local_path: row.get(3)?,
        build_command: row.get(4)?,
        artifact_path: row.get(5)?,
        pre_build_command: row.get(6)?,
        created_at: row.get(7)?,
    })
}

const PROJECT_COLS: &str =
    "id, name, kind, local_path, build_command, artifact_path, pre_build_command, created_at";

pub fn list_projects(conn: &Connection) -> Result<Vec<Project>, String> {
    let mut stmt = conn
        .prepare(&format!("SELECT {PROJECT_COLS} FROM project ORDER BY id"))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_project)
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())
}

pub fn get_project(conn: &Connection, id: i64) -> Result<Project, String> {
    conn.query_row(
        &format!("SELECT {PROJECT_COLS} FROM project WHERE id = ?1"),
        params![id],
        row_to_project,
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("项目不存在 (id={id})"))
}

pub fn save_project(conn: &Connection, input: &ProjectInput) -> Result<Project, String> {
    let id = match input.id {
        Some(id) => {
            let n = conn.execute(
                "UPDATE project SET name=?1, kind=?2, local_path=?3, build_command=?4, artifact_path=?5, pre_build_command=?6 WHERE id=?7",
                params![input.name, input.kind, input.local_path, input.build_command, input.artifact_path, input.pre_build_command, id],
            )
            .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err(format!("项目不存在 (id={id})"));
            }
            id
        }
        None => {
            conn.execute(
                "INSERT INTO project (name, kind, local_path, build_command, artifact_path, pre_build_command) VALUES (?1,?2,?3,?4,?5,?6)",
                params![input.name, input.kind, input.local_path, input.build_command, input.artifact_path, input.pre_build_command],
            )
            .map_err(|e| e.to_string())?;
            conn.last_insert_rowid()
        }
    };
    get_project(conn, id)
}

pub fn delete_project(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM project WHERE id=?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_profile(conn: &Connection, project_id: i64, server_id: i64, env_name: &str) -> Result<Option<DeployProfile>, String> {
    conn.query_row(
        "SELECT project_id, server_id, env_name, remote_dir, site_dir, jar_name, restart_command, health_check_url, backup_keep, is_prod
         FROM deploy_profile WHERE project_id=?1 AND server_id=?2 AND env_name=?3",
        params![project_id, server_id, env_name],
        |row| {
            Ok(DeployProfile {
                project_id: row.get(0)?,
                server_id: row.get(1)?,
                env_name: row.get(2)?,
                remote_dir: row.get(3)?,
                site_dir: row.get(4)?,
                jar_name: row.get(5)?,
                restart_command: row.get(6)?,
                health_check_url: row.get(7)?,
                backup_keep: row.get(8)?,
                is_prod: row.get::<_, i64>(9)? != 0,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn save_profile(conn: &Connection, input: &ProfileInput) -> Result<DeployProfile, String> {
    conn.execute(
        "INSERT INTO deploy_profile (project_id, server_id, env_name, remote_dir, site_dir, jar_name, restart_command, health_check_url, backup_keep, is_prod)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(project_id, server_id, env_name) DO UPDATE SET
           remote_dir=excluded.remote_dir, site_dir=excluded.site_dir, jar_name=excluded.jar_name, restart_command=excluded.restart_command,
           health_check_url=excluded.health_check_url, backup_keep=excluded.backup_keep, is_prod=excluded.is_prod",
        params![input.project_id, input.server_id, input.env_name, input.remote_dir, input.site_dir, input.jar_name, input.restart_command, input.health_check_url, input.backup_keep, input.is_prod as i64],
    )
    .map_err(|e| e.to_string())?;
    get_profile(conn, input.project_id, input.server_id, &input.env_name)?
        .ok_or_else(|| "保存部署配置失败".into())
}

pub fn list_environments(conn: &Connection, project_id: i64, server_id: i64) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT env_name FROM deploy_profile WHERE project_id=?1 AND server_id=?2 ORDER BY env_name")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![project_id, server_id], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<String>>>()
        .map_err(|e| e.to_string())
}

pub fn get_setting(conn: &Connection, key: &str, default: &str) -> String {
    conn.query_row("SELECT value FROM setting WHERE key=?1", params![key], |row| row.get::<_, String>(0))
        .unwrap_or_else(|_| default.to_string())
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO setting (key, value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn insert_record(
    conn: &Connection,
    project_id: i64,
    server_id: i64,
    env_name: &str,
    kind: &str,
    status: &str,
    started_at: &str,
    duration_ms: i64,
    failed_step: Option<&str>,
    message: &str,
    log_file: &str,
    backup_ref: Option<&str>,
    remote_dir: &str,
) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO deploy_record (project_id, server_id, env_name, kind, status, started_at, duration_ms, failed_step, message, log_file, backup_ref, remote_dir)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![project_id, server_id, env_name, kind, status, started_at, duration_ms, failed_step, message, log_file, backup_ref, remote_dir],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

fn row_to_record(row: &rusqlite::Row) -> rusqlite::Result<HistoryRecord> {
    Ok(HistoryRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        server_id: row.get(2)?,
        env_name: row.get(3)?,
        kind: row.get(4)?,
        status: row.get(5)?,
        started_at: row.get(6)?,
        duration_ms: row.get(7)?,
        failed_step: row.get(8)?,
        message: row.get(9)?,
        log_file: row.get(10)?,
        backup_ref: row.get(11)?,
        remote_dir: row.get(12)?,
    })
}

const RECORD_COLS: &str =
    "id, project_id, server_id, env_name, kind, status, started_at, duration_ms, failed_step, message, log_file, backup_ref, remote_dir";

pub fn list_records(conn: &Connection, project_id: Option<i64>) -> Result<Vec<HistoryRecord>, String> {
    let sql = match project_id {
        Some(_) => format!("SELECT {RECORD_COLS} FROM deploy_record WHERE project_id=?1 ORDER BY id DESC LIMIT 200"),
        None => format!("SELECT {RECORD_COLS} FROM deploy_record ORDER BY id DESC LIMIT 200"),
    };
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = match project_id {
        Some(id) => stmt.query_map(params![id], row_to_record),
        None => stmt.query_map([], row_to_record),
    }
    .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())
}

pub fn get_record(conn: &Connection, id: i64) -> Result<HistoryRecord, String> {
    conn.query_row(
        &format!("SELECT {RECORD_COLS} FROM deploy_record WHERE id=?1"),
        params![id],
        row_to_record,
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("记录不存在 (id={id})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bt_input() -> ServerInput {
        ServerInput {
            id: None,
            name: "宝塔面板".into(),
            host: "1.2.3.4".into(),
            port: 0,
            username: "".into(),
            auth_type: "apikey".into(),
            key_path: None,
            secret: None,
            server_type: "bt".into(),
            panel_url: Some("https://1.2.3.4:8888".into()),
            tls_insecure: true,
        }
    }

    #[test]
    fn bt_server_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();
        let id = insert_server(&conn, &bt_input()).unwrap();
        set_has_credential(&conn, id, true).unwrap();
        let s = get_server(&conn, id).unwrap();
        assert_eq!(s.server_type, "bt");
        assert_eq!(s.panel_url.as_deref(), Some("https://1.2.3.4:8888"));
        assert!(s.tls_insecure);
        assert!(s.has_credential);
        let mut upd = bt_input();
        upd.tls_insecure = false;
        update_server(&conn, id, &upd).unwrap();
        assert!(!get_server(&conn, id).unwrap().tls_insecure);
    }

    #[test]
    fn legacy_ssh_server_gets_defaults() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE server (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL, host TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 22,
                username TEXT NOT NULL, auth_type TEXT NOT NULL,
                key_path TEXT,
                has_credential INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
            );
            INSERT INTO server (name, host, port, username, auth_type)
                VALUES ('old', '10.0.0.1', 22, 'root', 'password');",
        )
        .unwrap();
        init(&conn).unwrap();
        let s = get_server(&conn, 1).unwrap();
        assert_eq!(s.server_type, "ssh");
        assert_eq!(s.panel_url, None);
        assert!(!s.tls_insecure);
    }
}