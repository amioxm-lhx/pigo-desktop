//! Import a pi CLI extension or skill package as a local development plugin
//! (spec 07-plugins/16 §3.2).
//!
//! The picker lives in the shell (D344), the source is copied under
//! `<dataDir>/plugins/imported/<slug>/src/`, and a generated manifest declares
//! exactly the contributions that were found. Import is explicit and local:
//! nothing scans `~/.pi`, nothing runs lifecycle scripts, and credential files
//! never travel with the copy.

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

const MAX_ENTRIES: usize = 8;
const MAX_SKILLS: usize = 32;
const INSTALL_TIMEOUT: Duration = Duration::from_secs(300);
const PROXY_VARS: [&str; 6] = [
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
];

#[derive(Debug)]
pub struct Imported {
    pub id: String,
    pub path: PathBuf,
    pub entries: Vec<String>,
    pub dependencies: Value,
}

/// Copy `source` into `<data_dir>/plugins/imported/<slug>` and write the
/// generated manifest. Dependency installation happens separately so a failed
/// install can be reported without blocking the import.
pub fn import(source: &Path, data_dir: &Path) -> Result<Imported, String> {
    let source = source
        .canonicalize()
        .map_err(|error| format!("无法解析选中的路径：{error}"))?;
    let base = if source.is_dir() {
        source
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
    } else {
        source
            .file_stem()
            .map(|name| name.to_string_lossy().to_string())
    }
    .unwrap_or_else(|| "extension".to_string());
    let slug = slugify(&base);

    let root = data_dir.join("plugins").join("imported");
    std::fs::create_dir_all(&root).map_err(|error| format!("无法创建导入目录：{error}"))?;
    let dest = unique_dir(&root, &slug);
    let dest_name = dest
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| slug.clone());
    std::fs::create_dir(&dest).map_err(|error| format!("无法创建插件目录：{error}"))?;

    let outcome = build(&source, &dest, &dest_name);
    match outcome {
        Ok((entries, _skills, package)) => {
            let dependencies = dependency_state(package.as_ref());
            Ok(Imported {
                id: format!("imported.{dest_name}"),
                path: dest,
                entries,
                dependencies,
            })
        }
        Err(error) => {
            // A partial copy is never left behind for the user to wonder about.
            let _ = std::fs::remove_dir_all(&dest);
            Err(error)
        }
    }
}

fn build(
    source: &Path,
    dest: &Path,
    slug: &str,
) -> Result<(Vec<String>, Vec<String>, Option<Value>), String> {
    let src_dir = dest.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|error| format!("无法创建 src 目录：{error}"))?;

    let mut entries: Vec<String> = Vec::new();
    let mut skills: Vec<String> = Vec::new();
    let mut package: Option<Value> = None;

    if source.is_dir() {
        let manifest_path = source.join("package.json");
        if manifest_path.is_file() {
            let raw = std::fs::read_to_string(&manifest_path)
                .map_err(|error| format!("无法读取 package.json：{error}"))?;
            package = serde_json::from_str::<Value>(&raw).ok();
        }
        copy_tree(source, &src_dir, true)?;
        if let Some(pkg) = &package {
            write_package_copy(pkg, source, dest)?;
            discover(&package, source, &mut entries, &mut skills)?;
        } else {
            discover(&None, source, &mut entries, &mut skills)?;
        }
    } else {
        let name = source
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .ok_or_else(|| "选中的文件没有名称".to_string())?;
        if !is_extension_file(&name) {
            return Err("选中的文件不是 .ts / .js / .mts / .mjs 扩展入口".to_string());
        }
        std::fs::copy(source, src_dir.join(&name))
            .map_err(|error| format!("无法复制扩展文件：{error}"))?;
        entries.push(format!("src/{name}"));
    }

    if entries.is_empty() && skills.is_empty() {
        return Err(
            "没有找到可导入的扩展入口（index.ts / index.js 或 package.json 的 pi.extensions）"
                .to_string(),
        );
    }

    write_noop_main(dest)?;
    write_manifest(dest, slug, &entries, &skills, package.as_ref())?;
    Ok((entries, skills, package))
}

fn write_noop_main(dest: &Path) -> Result<(), String> {
    let body = "/**\n * Imported pi extension — no plugin-side code.\n *\n * The extension entry itself is declared in manifest.json's\n * `contributes.agentExtensions`; this file only exists because a plugin\n * manifest must name a main module.\n */\n\nasync function onLoad() {}\n\nasync function onUnload() {}\n\nmodule.exports = { onLoad, onUnload };\n";
    std::fs::write(dest.join("main.js"), body).map_err(|error| format!("无法写入 main.js：{error}"))
}

fn write_manifest(
    dest: &Path,
    slug: &str,
    entries: &[String],
    skills: &[String],
    package: Option<&Value>,
) -> Result<(), String> {
    let name = title_case(slug);
    let version = package
        .and_then(|pkg| pkg.get("version"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .unwrap_or("0.1.0")
        .to_string();
    let mut contributes = json!({});
    let mut permissions: Vec<&str> = Vec::new();
    if !entries.is_empty() {
        contributes["agentExtensions"] = json!(entries);
        permissions.push("agent.extension");
    }
    if !skills.is_empty() {
        contributes["skills"] = json!(skills);
        permissions.push("agent.prompt.inject");
    }
    let manifest = json!({
        "schemaVersion": 1,
        "id": format!("imported.{slug}"),
        "name": name,
        "version": version,
        "description": format!("Imported pi extension {slug}"),
        "main": "main.js",
        "permissions": permissions,
        "contributes": contributes,
        "engines": { "piDesktop": ">=0.1.0" },
        "activationEvents": ["onStartup"],
    });
    let raw = serde_json::to_string_pretty(&manifest).unwrap_or_default() + "\n";
    std::fs::write(dest.join("manifest.json"), raw)
        .map_err(|error| format!("无法写入 manifest.json：{error}"))
}

/// The selected package's `package.json` and npm lockfile land at the plugin
/// root; `workspaces` is stripped so npm cannot adopt the original monorepo.
fn write_package_copy(package: &Value, source: &Path, dest: &Path) -> Result<(), String> {
    let mut copy = package.clone();
    if let Some(object) = copy.as_object_mut() {
        object.remove("workspaces");
    }
    let raw = serde_json::to_string_pretty(&copy).unwrap_or_default() + "\n";
    std::fs::write(dest.join("package.json"), raw)
        .map_err(|error| format!("无法写入 package.json：{error}"))?;
    for lock in ["package-lock.json", "npm-shrinkwrap.json"] {
        let path = source.join(lock);
        if path.is_file() {
            std::fs::copy(&path, dest.join(lock))
                .map_err(|error| format!("无法复制 {lock}：{error}"))?;
        }
    }
    Ok(())
}

fn discover(
    package: &Option<Value>,
    source: &Path,
    entries: &mut Vec<String>,
    skills: &mut Vec<String>,
) -> Result<(), String> {
    let declared_extensions = package
        .as_ref()
        .and_then(|pkg| pkg.pointer("/pi/extensions"))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let declared_skills = package
        .as_ref()
        .and_then(|pkg| pkg.pointer("/pi/skills"))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // A package that declares skills and no extensions is skill-only: its
    // scripts are resources, never executable agent extensions.
    if declared_extensions.is_empty() && !declared_skills.is_empty() {
        for declared in &declared_skills {
            expand_skill(source, declared, skills)?;
        }
        return Ok(());
    }
    if !declared_extensions.is_empty() {
        for declared in &declared_extensions {
            let relative = validate_relative(declared)?;
            let path = source.join(&relative);
            if !path.is_file() {
                return Err(format!("扩展入口不存在：{declared}"));
            }
            if !is_extension_file(declared) {
                return Err(format!("扩展入口必须是 .ts 或 .js 文件：{declared}"));
            }
            entries.push(relative_path(&path, source)?);
        }
        for declared in &declared_skills {
            expand_skill(source, declared, skills)?;
        }
        return Ok(());
    }

    // No declaration: index.ts / index.js, then loose scripts one level deep.
    for candidate in ["index.ts", "index.js"] {
        let path = source.join(candidate);
        if path.is_file() {
            entries.push(relative_path(&path, source)?);
            return Ok(());
        }
    }
    let mut loose: Vec<String> = std::fs::read_dir(source)
        .map_err(|error| format!("无法读取目录：{error}"))?
        .flatten()
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| is_extension_file(name))
        .collect();
    loose.sort();
    if loose.is_empty() {
        return Ok(());
    }
    if loose.len() > MAX_ENTRIES {
        return Err(format!(
            "扩展入口超过 {MAX_ENTRIES} 个，请在 package.json 的 pi.extensions 里显式声明"
        ));
    }
    for name in loose {
        entries.push(format!("src/{name}"));
    }
    Ok(())
}

/// A declared skill path is a Markdown document or a directory holding one.
fn expand_skill(source: &Path, declared: &str, skills: &mut Vec<String>) -> Result<(), String> {
    let relative = validate_relative(declared)?;
    let path = source.join(&relative);
    if !path.exists() {
        return Err(format!("技能路径不存在：{declared}"));
    }
    if path.is_file() {
        if !declared.to_ascii_lowercase().ends_with(".md") {
            return Err(format!("技能必须是 Markdown 文件：{declared}"));
        }
        push_skill(path, source, skills)?;
        return Ok(());
    }
    // A directory: its own SKILL.md wins; otherwise direct Markdown files are
    // included and subdirectories are searched for their own SKILL.md.
    let mut budget = 256usize;
    collect_skill_dir(&path, source, skills, &mut budget)?;
    Ok(())
}

/// Walk one skill directory. A directory holding `SKILL.md` contributes that
/// document and stops there, so support documents never become extra skills.
fn collect_skill_dir(
    dir: &Path,
    source: &Path,
    skills: &mut Vec<String>,
    budget: &mut usize,
) -> Result<(), String> {
    if *budget == 0 {
        return Err("技能目录扫描超过 256 个目录".to_string());
    }
    *budget -= 1;
    let own = dir.join("SKILL.md");
    if own.is_file() {
        push_skill(own, source, skills)?;
        return Ok(());
    }
    let mut docs: Vec<PathBuf> = Vec::new();
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir)
        .map_err(|error| format!("无法读取技能目录：{error}"))?
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            dirs.push(entry.path());
            continue;
        }
        if kind.is_file()
            && Path::new(&name)
                .extension()
                .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("md"))
                .unwrap_or(false)
        {
            docs.push(entry.path());
        }
    }
    docs.sort();
    dirs.sort();
    for doc in docs {
        push_skill(doc, source, skills)?;
    }
    for sub in dirs {
        collect_skill_dir(&sub, source, skills, budget)?;
    }
    Ok(())
}

fn push_skill(doc: PathBuf, source: &Path, skills: &mut Vec<String>) -> Result<(), String> {
    if skills.len() >= MAX_SKILLS {
        return Err(format!("技能数量超过 {MAX_SKILLS} 个"));
    }
    let relative = relative_path(&doc, source)?;
    if !skills.contains(&relative) {
        skills.push(relative);
    }
    Ok(())
}

/// Recursive copy with the exclusions spec 07-plugins/16 §3.2 requires.
fn copy_tree(source: &Path, target: &Path, root_level: bool) -> Result<(), String> {
    for entry in std::fs::read_dir(source)
        .map_err(|error| format!("无法读取 {}：{error}", source.display()))?
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().to_string();
        let kind = entry
            .file_type()
            .map_err(|error| format!("无法读取文件类型：{error}"))?;
        // Descendant symlinks are refused: they are a path out of the package.
        if kind.is_symlink() {
            return Err(format!("拒绝符号链接：{}", entry.path().display()));
        }
        if kind.is_dir() {
            if is_ignored_dir(&name) {
                continue;
            }
            let next = target.join(&name);
            std::fs::create_dir_all(&next).map_err(|error| format!("无法创建目录：{error}"))?;
            copy_tree(&entry.path(), &next, false)?;
            continue;
        }
        if !kind.is_file() {
            continue;
        }
        if is_credential_file(&name) {
            continue;
        }
        // The package manifest and lockfile are copied to the plugin root.
        if root_level
            && matches!(
                name.as_str(),
                "package.json" | "package-lock.json" | "npm-shrinkwrap.json"
            )
        {
            continue;
        }
        let next = target.join(&name);
        std::fs::copy(entry.path(), &next)
            .map_err(|error| format!("无法复制 {}：{error}", entry.path().display()))?;
    }
    Ok(())
}

fn is_ignored_dir(name: &str) -> bool {
    matches!(name, "node_modules" | ".git" | ".hg" | ".svn" | ".DS_Store")
}

fn is_credential_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == ".npmrc"
        || lower == ".netrc"
        || lower == ".pypirc"
        || lower.starts_with(".env")
        || lower.starts_with("id_rsa")
        || lower.starts_with("id_ed25519")
        || [".pem", ".key", ".p12", ".pfx", ".crt", ".cer"]
            .iter()
            .any(|extension| lower.ends_with(extension))
}

fn is_extension_file(name: &str) -> bool {
    let path = Path::new(name);
    path.extension()
        .map(|extension| {
            matches!(
                extension.to_string_lossy().to_ascii_lowercase().as_str(),
                "ts" | "js" | "mts" | "mjs"
            )
        })
        .unwrap_or(false)
}

/// Relative, forward-slash path inside the selected package, or a refusal.
fn validate_relative(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim().replace('\\', "/");
    if trimmed.is_empty() || trimmed.starts_with('/') {
        return Err(format!("必须是包内相对路径：{raw}"));
    }
    let path = Path::new(&trimmed);
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(format!("路径不能越出所选包：{raw}"));
    }
    if trimmed.split('/').any(|segment| segment == "node_modules") {
        return Err(format!("路径不能指向依赖目录：{raw}"));
    }
    Ok(path.to_path_buf())
}

fn relative_path(path: &Path, source: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(source)
        .map_err(|_| format!("路径越出所选包：{}", path.display()))?;
    Ok(format!(
        "src/{}",
        relative.to_string_lossy().replace('\\', "/")
    ))
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for ch in value.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "extension".to_string()
    } else {
        trimmed
    }
}

fn title_case(slug: &str) -> String {
    slug.split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn unique_dir(root: &Path, slug: &str) -> PathBuf {
    let first = root.join(slug);
    if !first.exists() {
        return first;
    }
    for index in 2..1000 {
        let candidate = root.join(format!("{slug}-{index}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    root.join(format!("{slug}-{}", uuid::Uuid::new_v4()))
}

/// The renderer's DependencyState: skipped / pending / installed / failed.
/// `pending` means the import declared installable dependencies and the caller
/// still owes the bounded npm step.
fn dependency_state(package: Option<&Value>) -> Value {
    let Some(package) = package else {
        return json!({ "state": "skipped", "reason": "no-package-json" });
    };
    for field in [
        "dependencies",
        "optionalDependencies",
        "devDependencies",
        "peerDependencies",
    ] {
        let Some(map) = package.get(field).and_then(Value::as_object) else {
            continue;
        };
        for (name, value) in map {
            let spec = value.as_str().unwrap_or("").trim().to_string();
            if !is_registry_spec(&spec) {
                return json!({
                    "state": "failed",
                    "error": format!("依赖 {name} 使用了非 registry 规格：{spec}"),
                });
            }
        }
    }
    let installs = ["dependencies", "optionalDependencies"]
        .iter()
        .any(|field| {
            package
                .get(*field)
                .and_then(Value::as_object)
                .map(|map| !map.is_empty())
                .unwrap_or(false)
        });
    if !installs {
        return json!({ "state": "skipped", "reason": "no-dependencies" });
    }
    json!({ "state": "pending" })
}

/// Registry-only specs: everything else could execute or reach the network
/// through a resolver npm would call.
fn is_registry_spec(spec: &str) -> bool {
    let value = spec.trim();
    if value.is_empty() {
        return true;
    }
    if value.starts_with('.')
        || value.starts_with('/')
        || value.contains("://")
        || value.starts_with("file:")
        || value.starts_with("link:")
        || value.starts_with("portal:")
        || value.starts_with("workspace:")
        || value.starts_with("git")
        || value.starts_with("github:")
        || value.starts_with("gitlab:")
        || value.starts_with("bitbucket:")
        || value.starts_with("http:")
        || value.starts_with("https:")
    {
        return false;
    }
    true
}

/// Two-step bounded install with scripts disabled and npm's cache/config
/// isolated from the user's environment (spec §3.2).
pub async fn install_dependencies(dir: &Path, dependencies: &Value) -> Value {
    if dependencies.get("state").and_then(Value::as_str) != Some("pending") {
        return dependencies.clone();
    }
    let mut first = npm_command(dir);
    first.args([
        "install",
        "--package-lock-only",
        "--omit=dev",
        "--legacy-peer-deps",
        "--no-audit",
        "--no-fund",
        "--ignore-scripts",
    ]);
    if let Err(error) = run_npm(first).await {
        cleanup_failed_install(dir);
        return json!({ "state": "failed", "error": error });
    }
    let mut second = npm_command(dir);
    second.args([
        "ci",
        "--omit=dev",
        "--legacy-peer-deps",
        "--no-audit",
        "--no-fund",
        "--ignore-scripts",
    ]);
    if let Err(error) = run_npm(second).await {
        cleanup_failed_install(dir);
        return json!({ "state": "failed", "error": error });
    }
    // The isolated npm config/cache are scaffolding, not plugin content.
    let _ = std::fs::remove_file(dir.join(".npmrc-ci"));
    let _ = std::fs::remove_dir_all(dir.join(".npm-cache"));
    json!({ "state": "installed" })
}

fn npm_command(dir: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("npm");
    command
        .current_dir(dir)
        .env("npm_config_cache", dir.join(".npm-cache"))
        .env("npm_config_userconfig", dir.join(".npmrc-ci"))
        .env("npm_config_ignore_scripts", "true")
        .env("npm_config_audit", "false")
        .env("npm_config_fund", "false")
        .env("npm_config_update_notifier", "false");
    for key in PROXY_VARS {
        command.env_remove(key);
    }
    command
}

async fn run_npm(mut command: tokio::process::Command) -> Result<(), String> {
    let output = tokio::time::timeout(INSTALL_TIMEOUT, command.output())
        .await
        .map_err(|_| "npm 安装超时（5 分钟）".to_string())?
        .map_err(|error| format!("无法运行 npm：{error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let tail: String = stderr
        .lines()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!("npm 退出码 {}：{tail}", output.status))
}

fn cleanup_failed_install(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir.join("node_modules"));
    let _ = std::fs::remove_dir_all(dir.join(".npm-cache"));
    let _ = std::fs::remove_file(dir.join(".npmrc-ci"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pi-import-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_package_with_pi_extensions_imports_under_src() {
        let source = temp_dir("ext");
        std::fs::write(
            source.join("package.json"),
            r#"{"name":"demo","version":"1.2.0","pi":{"extensions":["./addon.ts"]}}"#,
        )
        .unwrap();
        std::fs::write(source.join("addon.ts"), "export default {}").unwrap();
        let data = temp_dir("data");
        let imported = import(&source, &data).expect("import");
        // The temp directory name decides the slug, so the id is derived from
        // it rather than from the package's own name.
        assert!(imported.id.starts_with("imported."));
        assert_eq!(imported.entries, vec!["src/addon.ts".to_string()]);
        let manifest: Value = serde_json::from_str(
            &std::fs::read_to_string(imported.path.join("manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["permissions"], json!(["agent.extension"]));
        assert_eq!(manifest["version"], "1.2.0");
        assert!(imported.path.join("src/addon.ts").is_file());
        assert!(imported.path.join("main.js").is_file());
        let _ = std::fs::remove_dir_all(source);
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn credential_files_and_node_modules_stay_out_of_the_copy() {
        let source = temp_dir("secrets");
        std::fs::write(source.join("index.js"), "module.exports = {}").unwrap();
        std::fs::write(source.join(".env"), "TOKEN=1").unwrap();
        std::fs::write(source.join("server.pem"), "-----BEGIN").unwrap();
        std::fs::create_dir_all(source.join("node_modules/dep")).unwrap();
        std::fs::write(source.join("node_modules/dep/index.js"), "").unwrap();
        let data = temp_dir("data");
        let imported = import(&source, &data).expect("import");
        assert!(!imported.path.join("src/.env").exists());
        assert!(!imported.path.join("src/server.pem").exists());
        assert!(!imported.path.join("src/node_modules").exists());
        assert!(imported.path.join("src/index.js").is_file());
        let _ = std::fs::remove_dir_all(source);
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn skill_only_packages_contribute_skills_without_extensions() {
        let source = temp_dir("skills");
        std::fs::write(
            source.join("package.json"),
            r#"{"pi":{"skills":["skills"]}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(source.join("skills/writer")).unwrap();
        std::fs::write(
            source.join("skills/writer/SKILL.md"),
            "---\nname: Writer\n---\n",
        )
        .unwrap();
        let data = temp_dir("data");
        let imported = import(&source, &data).expect("import");
        assert!(imported.entries.is_empty());
        let manifest: Value = serde_json::from_str(
            &std::fs::read_to_string(imported.path.join("manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["permissions"], json!(["agent.prompt.inject"]));
        assert_eq!(
            manifest["contributes"]["skills"][0],
            "src/skills/writer/SKILL.md"
        );
        let _ = std::fs::remove_dir_all(source);
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn a_package_without_an_entry_fails_instead_of_half_importing() {
        let source = temp_dir("empty");
        std::fs::write(source.join("README.md"), "nothing here").unwrap();
        let data = temp_dir("data");
        let error = import(&source, &data).expect_err("should refuse");
        assert!(error.contains("没有找到可导入的扩展入口"));
        assert!(!data.join("plugins/imported/empty").exists());
        let _ = std::fs::remove_dir_all(source);
        let _ = std::fs::remove_dir_all(data);
    }
}
