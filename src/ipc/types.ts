export interface Server {
  id: number;
  name: string;
  host: string;
  port: number;
  username: string;
  auth_type: "password" | "key";
  key_path: string | null;
  has_credential: boolean;
  created_at: string;
}

export interface ServerInput {
  id?: number;
  name: string;
  host: string;
  port: number;
  username: string;
  auth_type: "password" | "key";
  key_path: string | null;
  secret?: string;
}

export interface Project {
  id: number;
  name: string;
  type: "frontend" | "java";
  local_path: string;
  build_command: string;
  artifact_path: string;
  pre_build_command: string | null;
  created_at: string;
}

export interface ProjectInput {
  id?: number;
  name: string;
  type: "frontend" | "java";
  local_path: string;
  build_command: string;
  artifact_path: string;
  pre_build_command: string | null;
}

export interface TestResult {
  ok: boolean;
  latency_ms: number;
  os_info: string | null;
  message: string;
}

export interface DetectResult {
  type: "frontend" | "java" | null;
  build_command: string | null;
  artifact_path: string | null;
  name: string | null;
}

export type DeployEvent =
  | { type: "step"; project_id: number; step: string; status: "running" | "done" | "failed"; detail: string | null }
  | { type: "log"; project_id: number; line: string; level: "info" | "stdout" | "stderr" | "error" | "ok" }
  | { type: "progress"; project_id: number; uploaded: number; total: number; speed_bps: number }
  | { type: "finished"; project_id: number; ok: boolean; cancelled: boolean; message: string; duration_ms: number };

export interface DeployProfile {
  project_id: number;
  server_id: number;
  env_name: string;
  remote_dir: string;
  site_dir: string;
  jar_name: string;
  restart_command: string;
  health_check_url: string;
  backup_keep: number;
  is_prod: boolean;
}

export interface HistoryRecord {
  id: number;
  project_id: number;
  server_id: number;
  env_name: string;
  kind: "deploy" | "rollback";
  status: "success" | "failed" | "cancelled";
  started_at: string;
  duration_ms: number;
  failed_step: string | null;
  message: string | null;
  log_file: string | null;
  backup_ref: string | null;
  remote_dir: string | null;
}