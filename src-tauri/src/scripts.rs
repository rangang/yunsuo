//! 远程部署 / 回滚 shell 脚本模板。
//! 原则：临时目录上传 → 备份旧版 → 原子替换 → 重启 → 清理；成功判据为退出码 0 且输出含 DEPLOY_OK。
//! 注意：文件操作用 if 语句（避免 set -e 下 [ ] && cmd 条件为假时静默退出）；
//! 重启命令在 set +e 下执行并显式报告退出码（避免 kill 失败等被静默吞掉）。
use crate::model::{DeployProfile, Project};

/// shell 单引号安全转义（仅用于独立 shell 词，如变量赋值）
fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn cleanup_backups(keep: i64) -> String {
    let n = keep.max(1);
    format!(
        "ls -1t \"$APP_DIR/.backups\" 2>/dev/null | tail -n +{} | while read -r f; do rm -f \"$APP_DIR/.backups/$f\"; done\n",
        n + 1
    )
}

/// 重启命令包装：set +e 执行 → 捕获退出码 → 失败显式报错
fn restart_block(restart: &str) -> String {
    if restart.is_empty() {
        return String::new();
    }
    format!(
        "set +e\n{restart}\nRESTART_CODE=$?\nset -e\n\
         if [ \"$RESTART_CODE\" -ne 0 ]; then echo \"RESTART_FAILED code=$RESTART_CODE\"; exit \"$RESTART_CODE\"; fi\n\
         echo \"[deploy] restart ok\"\n"
    )
}

pub fn deploy_script(project: &Project, profile: &DeployProfile, ts: &str, upload_name: &str) -> String {
    let dir = q(&profile.remote_dir);
    let site = q(&profile.site_dir);
    let restart = profile.restart_command.trim();
    let mut s = String::new();
    s.push_str("set -e\n");
    s.push_str(&format!("APP_DIR={dir}\nSITE={site}\nTS={}\n", q(ts)));
    s.push_str("mkdir -p \"$APP_DIR/.backups\"\n");
    if project.kind == "frontend" {
        s.push_str(
            "if [ -d \"$APP_DIR/$SITE\" ]; then tar -czf \"$APP_DIR/.backups/$TS.tar.gz\" -C \"$APP_DIR\" \"$SITE\"; fi\n\
             unzip -q \"$APP_DIR/.deploy_tmp/$TS/dist.zip\" -d \"$APP_DIR/.deploy_tmp/$TS/new\"\n\
             rm -rf \"$APP_DIR/$SITE.old\"\n\
             if [ -d \"$APP_DIR/$SITE\" ]; then mv \"$APP_DIR/$SITE\" \"$APP_DIR/$SITE.old\"; fi\n\
             mv \"$APP_DIR/.deploy_tmp/$TS/new\" \"$APP_DIR/$SITE\"\n\
             rm -rf \"$APP_DIR/.deploy_tmp/$TS\"\n\
             echo \"[deploy] site replaced: $SITE\"\n",
        );
    } else {
        // jar_name / upload_name 已校验为安全字符（字母数字 . _ -），双引号内直接嵌入
        s.push_str(&format!(
            "if [ -f \"$APP_DIR/{jar}\" ]; then cp \"$APP_DIR/{jar}\" \"$APP_DIR/.backups/app_$TS.jar\"; fi\n\
             mv \"$APP_DIR/.deploy_tmp/$TS/{upload}\" \"$APP_DIR/{jar}\"\n\
             rm -rf \"$APP_DIR/.deploy_tmp/$TS\"\n\
             echo \"[deploy] jar replaced: {jar}\"\n",
            jar = profile.jar_name,
            upload = upload_name
        ));
    }
    s.push_str(&cleanup_backups(profile.backup_keep));
    s.push_str(&restart_block(restart));
    s.push_str("echo \"DEPLOY_OK\"\n");
    s
}

/// backup_ts = None 时回滚到最新备份
pub fn rollback_script(project: &Project, profile: &DeployProfile, backup_ts: Option<&str>) -> String {
    let dir = q(&profile.remote_dir);
    let site = q(&profile.site_dir);
    let restart = profile.restart_command.trim();
    let mut s = String::new();
    s.push_str("set -e\n");
    s.push_str(&format!("APP_DIR={dir}\nSITE={site}\n"));
    if project.kind == "frontend" {
        match backup_ts {
            Some(ts) => s.push_str(&format!("BACKUP={}\n", q(&format!("{ts}.tar.gz")))),
            None => s.push_str("BACKUP=$(ls -1t \"$APP_DIR/.backups\" 2>/dev/null | head -n 1)\n"),
        }
        s.push_str(
            "if [ -z \"$BACKUP\" ] || [ ! -f \"$APP_DIR/.backups/$BACKUP\" ]; then echo \"NO_BACKUP\"; exit 1; fi\n\
             rm -rf \"$APP_DIR/$SITE.failed\"\n\
             if [ -d \"$APP_DIR/$SITE\" ]; then mv \"$APP_DIR/$SITE\" \"$APP_DIR/$SITE.failed\"; fi\n\
             tar -xzf \"$APP_DIR/.backups/$BACKUP\" -C \"$APP_DIR\"\n\
             rm -rf \"$APP_DIR/$SITE.failed\"\n\
             echo \"[rollback] site restored: $BACKUP\"\n",
        );
    } else {
        match backup_ts {
            Some(ts) => s.push_str(&format!("BACKUP={}\n", q(&format!("app_{ts}.jar")))),
            None => s.push_str("BACKUP=$(ls -1t \"$APP_DIR/.backups\" 2>/dev/null | grep '^app_' | head -n 1)\n"),
        }
        s.push_str(&format!(
            "if [ -z \"$BACKUP\" ] || [ ! -f \"$APP_DIR/.backups/$BACKUP\" ]; then echo \"NO_BACKUP\"; exit 1; fi\n\
             cp \"$APP_DIR/.backups/$BACKUP\" \"$APP_DIR/{jar}\"\n\
             echo \"[rollback] jar restored: $BACKUP\"\n",
            jar = profile.jar_name
        ));
    }
    s.push_str(&restart_block(restart));
    s.push_str("echo \"DEPLOY_OK\"\n");
    s
}

pub fn health_check_script(url: &str) -> String {
    format!(
        "curl -s -o /dev/null -w '%{{http_code}}' --max-time 10 {}",
        q(url)
    )
}