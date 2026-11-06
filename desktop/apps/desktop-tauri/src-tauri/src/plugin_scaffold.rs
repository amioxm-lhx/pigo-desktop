//! Plugin templates for the Plugins page (spec 07-plugins/10 §3).
//!
//! The Electron build called `@pi-desktop/plugin-devkit`'s `scaffold()` from
//! the main process. The Tauri shell cannot import that TypeScript package, so
//! the four official templates are generated here, byte-compatible in shape:
//! the same ids, the same files, and the same manifest contributions.
//!
//! Scaffolding is create-only: it refuses a non-empty directory, because
//! silently merging into existing sources is how people lose work.

use std::path::{Path, PathBuf};

use serde_json::json;

pub const TEMPLATES: [&str; 4] = ["panel-basic", "agent-tool-basic", "skill-pack", "full-demo"];

pub struct ScaffoldResult {
    pub dir: PathBuf,
    pub id: String,
    pub name: String,
    pub files: Vec<String>,
}

struct Vars {
    id: String,
    name: String,
    slug: String,
    version: String,
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
        "plugin".to_string()
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

/// Write a starter plugin into `dir` and report what it wrote.
pub fn scaffold(dir: &Path, template: &str) -> Result<ScaffoldResult, String> {
    if !TEMPLATES.contains(&template) {
        return Err(format!(
            "unknown template \"{template}\" (expected one of: {})",
            TEMPLATES.join(", ")
        ));
    }
    std::fs::create_dir_all(dir).map_err(|error| format!("无法创建目录：{error}"))?;
    let dir = dir
        .canonicalize()
        .map_err(|error| format!("无法解析目录：{error}"))?;
    let occupied = std::fs::read_dir(&dir)
        .map_err(|error| format!("无法读取目录：{error}"))?
        .flatten()
        .any(|entry| entry.file_name().to_string_lossy() != ".DS_Store");
    if occupied {
        return Err(format!("目录不是空目录：{}", dir.display()));
    }

    let slug = slugify(
        dir.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .as_deref()
            .unwrap_or("plugin"),
    );
    let id = format!("local.{slug}");
    let id_re =
        regex::Regex::new(r"^[a-z0-9]+(\.[a-z0-9_-]+)+$").map_err(|error| error.to_string())?;
    if !id_re.is_match(&id) {
        return Err(format!("插件 id \"{id}\" 不合法"));
    }
    let vars = Vars {
        id,
        name: title_case(&slug),
        slug,
        version: "0.1.0".to_string(),
    };

    let files = template_files(template, &vars)?;
    for (relative, content) in &files {
        let target = dir.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("无法创建目录：{error}"))?;
        }
        std::fs::write(&target, content)
            .map_err(|error| format!("无法写入 {}：{error}", target.display()))?;
    }
    Ok(ScaffoldResult {
        dir,
        id: vars.id,
        name: vars.name,
        files: files.into_iter().map(|(relative, _)| relative).collect(),
    })
}

fn template_files(template: &str, vars: &Vars) -> Result<Vec<(String, String)>, String> {
    let panel = template == "panel-basic" || template == "full-demo";
    let skill = template == "skill-pack" || template == "full-demo";
    let mut files: Vec<(String, String)> = vec![
        ("manifest.json".into(), manifest_json(template, vars)),
        ("main.js".into(), main_js(template, vars)),
    ];
    if panel {
        files.push(("renderer/index.html".into(), panel_html(vars)));
    }
    if skill {
        files.push((format!("skills/{}.md", vars.slug), skill_doc(vars)));
    }
    files.push(("README.md".into(), readme(template, vars, panel)));
    Ok(files)
}

fn manifest_json(template: &str, vars: &Vars) -> String {
    let panel = template == "panel-basic" || template == "full-demo";
    let tool = template == "agent-tool-basic" || template == "full-demo";
    let skill = template == "skill-pack" || template == "full-demo";

    let mut contributes = json!({});
    let mut permissions: Vec<&str> = Vec::new();
    let mut activation: Vec<String> = vec!["onStartup".into()];

    if panel {
        contributes["commands"] = json!([{
            "id": format!("{}.open", vars.slug),
            "title": format!("{}: Open Panel", vars.name),
            "keywords": [vars.slug],
        }]);
        permissions.push("ui.panel");
        activation.insert(0, format!("onCommand:{}.open", vars.slug));
    }
    if tool {
        contributes["agentTools"] = json!([{
            "name": "echo_text",
            "description": "Echo text back to the agent.",
            "risk": "low",
            "schema": {
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"],
            },
        }]);
        permissions.push("agent.tool.register");
    }
    if skill {
        contributes["skills"] = json!([format!("skills/{}.md", vars.slug)]);
        permissions.push("agent.prompt.inject");
    }
    if template == "full-demo" {
        let mut commands = contributes["commands"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        commands.push(json!({
            "id": format!("{}.write-note", vars.slug),
            "title": format!("{}: Write demo note", vars.name),
            "keywords": [vars.slug, "note"],
        }));
        commands.push(json!({
            "id": format!("{}.clean-note", vars.slug),
            "title": format!("{}: Delete demo note", vars.name),
            "keywords": [vars.slug, "clean"],
        }));
        contributes["commands"] = json!(commands);
        permissions.push("fs.write");
        permissions.push("fs.delete");
        activation.insert(0, format!("onCommand:{}.write-note", vars.slug));
        activation.insert(0, format!("onCommand:{}.clean-note", vars.slug));
        contributes["settings"] = json!([
            { "key": "greeting", "type": "string", "default": format!("Hello from {}", vars.name), "title": "Greeting" },
            {
                "key": "openShortcut",
                "type": "shortcut",
                "default": "Mod+Shift+H",
                "title": "Open panel shortcut",
                "command": format!("{}.open", vars.slug),
            },
        ]);
    }

    let mut manifest = json!({
        "schemaVersion": 1,
        "id": vars.id,
        "name": vars.name,
        "version": vars.version,
        "description": format!("{} — generated from the {template} template.", vars.name),
        "main": "main.js",
        "contributes": contributes,
        "permissions": permissions,
        "engines": { "piDesktop": ">=0.1.0" },
        "activationEvents": activation,
    });
    if panel {
        manifest["ui"] = json!({ "panel": "renderer/index.html", "title": vars.name });
    }
    if template == "full-demo" {
        manifest["fs"] = json!({
            "write": { "root": "workspace", "scope": ["out/**"] },
            "delete": { "own": true },
        });
    }
    serde_json::to_string_pretty(&manifest).unwrap_or_default() + "\n"
}

fn main_js(template: &str, vars: &Vars) -> String {
    let panel = template == "panel-basic" || template == "full-demo";
    let tool = template == "agent-tool-basic" || template == "full-demo";
    let mut load: Vec<String> = Vec::new();
    let mut unload: Vec<String> = Vec::new();

    if template == "full-demo" {
        load.push("  const settings = await pi.plugin.getSettings();".into());
    }
    if panel {
        let toast = if template == "full-demo" {
            format!("settings.greeting || \"Hello from {}\"", vars.name)
        } else {
            format!("\"Hello from {}\"", vars.name)
        };
        load.push("  await pi.commands.register({".into());
        load.push(format!("    id: \"{}.open\",", vars.slug));
        load.push(format!("    title: \"{}: Open Panel\",", vars.name));
        load.push(format!("    keywords: [\"{}\"],", vars.slug));
        load.push("    run: async () => {".into());
        load.push(format!(
            "      await pi.ui.openPanel({{ title: \"{}\" }});",
            vars.name
        ));
        load.push(format!("      await pi.ui.showToast({toast});"));
        load.push("    },".into());
        load.push("  });".into());
        unload.push(format!(
            "  await pi.commands.unregister(\"{}.open\");",
            vars.slug
        ));
    }
    if tool {
        load.push("  await pi.agent.registerTool({".into());
        load.push("    name: \"echo_text\",".into());
        load.push("    description: \"Echo text back to the agent.\",".into());
        load.push("    risk: \"low\",".into());
        load.push("    schema: {".into());
        load.push("      type: \"object\",".into());
        load.push("      properties: { text: { type: \"string\" } },".into());
        load.push("      required: [\"text\"],".into());
        load.push("    },".into());
        load.push("    execute: async (args) => ({".into());
        load.push("      ok: true,".into());
        load.push("      echo: String(args?.text ?? \"\"),".into());
        load.push("      pluginId: pi.plugin.getId(),".into());
        load.push("    }),".into());
        load.push("  });".into());
        unload.push("  await pi.agent.unregisterTool(\"echo_text\");".into());
    }
    if template == "full-demo" {
        let note = format!("out/{}-note.md", vars.slug);
        load.push("  await pi.commands.register({".into());
        load.push(format!("    id: \"{}.write-note\",", vars.slug));
        load.push(format!("    title: \"{}: Write demo note\",", vars.name));
        load.push(format!("    keywords: [\"{}\", \"note\"],", vars.slug));
        load.push("    run: async () => {".into());
        load.push(format!(
            "      await pi.fs.writeText(\"{note}\", \"# {}\\n\\nWritten by the plugin.\\n\");",
            vars.name
        ));
        load.push(format!("      await pi.ui.showToast(\"Wrote {note}\");"));
        load.push("    },".into());
        load.push("  });".into());
        load.push("  await pi.commands.register({".into());
        load.push(format!("    id: \"{}.clean-note\",", vars.slug));
        load.push(format!("    title: \"{}: Delete demo note\",", vars.name));
        load.push(format!("    keywords: [\"{}\", \"clean\"],", vars.slug));
        load.push("    run: async () => {".into());
        load.push(format!("      await pi.fs.remove(\"{note}\");"));
        load.push(format!(
            "      await pi.ui.showToast(\"Moved {note} to the trash\");"
        ));
        load.push("    },".into());
        load.push("  });".into());
        unload.push(format!(
            "  await pi.commands.unregister(\"{}.write-note\");",
            vars.slug
        ));
        unload.push(format!(
            "  await pi.commands.unregister(\"{}.clean-note\");",
            vars.slug
        ));
    }
    if load.is_empty() {
        // skill-pack contributes prompt text only; the entry still has to exist
        // because host-core rejects a manifest whose `main` is missing.
        load.push(format!(
            "  await pi.ui.showToast(\"{} skills are active.\");",
            vars.name
        ));
    }

    format!(
        "/**\n * {} — PI-Desktop plugin entry.\n *\n * The host injects the global `pi` object. Every call is gated by the\n * permissions declared in manifest.json, so widening what this file does\n * usually means widening `permissions` too.\n */\n\nasync function onLoad() {{\n{}\n}}\n\nasync function onUnload() {{\n{}\n}}\n\nmodule.exports = {{ onLoad, onUnload }};\n",
        vars.name,
        load.join("\n"),
        if unload.is_empty() {
            "  // nothing to tear down".to_string()
        } else {
            unload.join("\n")
        }
    )
}

fn panel_html(vars: &Vars) -> String {
    let html = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <meta name="pi-plugin-chrome" content="v2" />
    <title>__NAME__</title>
    <style>
      :root {
        color-scheme: dark;
        --bg: #181818;
        --surface: #212121;
        --fg: #ffffff;
        --muted: color-mix(in oklab, #ffffff 52%, transparent);
        --border: color-mix(in oklab, #ffffff 10%, transparent);
        --accent: #ffffff;
      }
      :root[data-base="light"] {
        color-scheme: light;
        --bg: #ffffff;
        --surface: #f9f9f9;
        --fg: #1a1c1f;
        --muted: #5d5d5d;
        --border: color-mix(in oklab, #1a1c1f 10%, transparent);
        --accent: #1a1c1f;
      }
      * { box-sizing: border-box; }
      body {
        margin: 0;
        min-height: 100vh;
        padding: var(--pi-plugin-titlebar-height, 0px) 16px 16px;
        overflow: auto;
        font: 13px/1.45 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        background: var(--bg);
        color: var(--fg);
      }
      .card {
        border: 1px solid var(--border);
        border-radius: 12px;
        padding: 16px;
        background: var(--surface);
      }
      h2 { margin: 0 0 4px; font-size: 16px; font-weight: 560; letter-spacing: -0.02em; }
      p { margin: 0; color: var(--muted); }
      button {
        margin-top: 12px;
        border: 1px solid var(--border);
        border-radius: 8px;
        padding: 8px 12px;
        background: var(--accent);
        color: var(--bg);
        cursor: pointer;
        font: inherit;
      }
      button:hover { opacity: 0.82; }
    </style>
  </head>
  <body>
    <div class="card">
      <h2>__NAME__</h2>
      <p>Panel served from this plugin's renderer directory.</p>
      <button id="ping">Toast ping</button>
    </div>
    <script>
      const applyAppearance = (appearance) => {
        const base = appearance?.base;
        document.documentElement.dataset.base =
          base === "light" || base === "dark"
            ? base
            : window.matchMedia("(prefers-color-scheme: light)").matches
              ? "light"
              : "dark";
      };
      window.pluginBridge?.on("appearance:changed", applyAppearance);
      window.pluginBridge?.invoke("app.getAppearance").then(applyAppearance).catch(() => applyAppearance(null));
      document.getElementById("ping").addEventListener("click", async () => {
        if (window.pluginBridge?.invoke) {
          await window.pluginBridge.invoke("ui.showToast", { message: "__NAME__ panel bridge" });
        } else {
          alert("pluginBridge is unavailable outside PI-Desktop");
        }
      });
    </script>
  </body>
</html>
"#;
    html.replace("__NAME__", &vars.name)
}

fn skill_doc(vars: &Vars) -> String {
    format!(
        "---\nname: {name}\ndescription: Describe when the agent should follow this skill.\n---\n\n# {name}\n\nReplace this body with the instructions the agent should follow. Keep it short\nand specific — the whole document is injected into the system prompt whenever\nthis plugin is enabled, and it competes for context with the user's own\n`AGENTS.md`.\n\n## When to use\n\n- Describe the situations this skill applies to.\n\n## How to use\n\n- Describe the concrete steps, and name the tools this plugin registers.\n",
        name = vars.name
    )
}

fn readme(template: &str, vars: &Vars, panel: bool) -> String {
    let mut contributions: Vec<String> = Vec::new();
    if panel {
        contributions.push(format!(
            "- Command `{}.open` opening a panel from `renderer/index.html`",
            vars.slug
        ));
    }
    if template == "agent-tool-basic" || template == "full-demo" {
        contributions.push("- Agent tool `echo_text`".to_string());
    }
    if template == "skill-pack" || template == "full-demo" {
        contributions.push(format!("- Skill `skills/{}.md`", vars.slug));
    }
    if template == "full-demo" {
        contributions.push("- Setting `greeting`".to_string());
        contributions.push(format!(
            "- Commands `{0}.write-note` / `{0}.clean-note`, writing `out/{0}-note.md` and moving it to the trash",
            vars.slug
        ));
    }
    format!(
        "# {name}\n\nGenerated from the `{template}` template.\n\n## Contributions\n\n{contributions}\n\n## Develop\n\n1. Open the Plugins page and use **Load development plugin**, pointing at this\n   directory. PI-Desktop reloads the plugin whenever you save a file here.\n2. Verify the contributions from the command palette.\n3. Validate and package:\n\n```bash\npnpm pi-plugin check .\npnpm pi-plugin pack .\n# writes dist/{id}-{version}.piplug\n```\n\n## Panel top drag band\n\nPI-Desktop reserves exactly a transparent 46px frameless drag band above panel\ncontent. Normal-flow content is offset automatically; for `position: fixed` or\n`sticky` content use `top: var(--pi-plugin-titlebar-height, 46px)`.\n",
        name = vars.name,
        template = template,
        contributions = contributions.join("\n"),
        id = vars.id,
        version = vars.version,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pi-scaffold-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn each_template_writes_the_files_the_manifest_names() {
        for template in TEMPLATES {
            let root = temp_dir(template);
            let result = scaffold(&root, template).expect("scaffold");
            assert_eq!(
                result.id,
                format!("local.{}", result.name.to_lowercase().replace(' ', "-"))
            );
            let manifest: Value =
                serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).unwrap())
                    .unwrap();
            assert_eq!(manifest["schemaVersion"], 1);
            let main = root.join(manifest["main"].as_str().unwrap());
            assert!(main.exists(), "{template} needs its main entry");
            if let Some(panel) = manifest["ui"]["panel"].as_str() {
                assert!(root.join(panel).exists(), "{template} panel must exist");
            }
            for skill in manifest["contributes"]["skills"]
                .as_array()
                .into_iter()
                .flatten()
            {
                assert!(root.join(skill.as_str().unwrap()).exists());
            }
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn a_non_empty_directory_is_refused() {
        let root = temp_dir("occupied");
        std::fs::write(root.join("keep.txt"), "user work").unwrap();
        assert!(scaffold(&root, "panel-basic").is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unknown_templates_are_refused() {
        let root = temp_dir("unknown");
        assert!(scaffold(&root, "nope").is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
