//! Converter for `supervisord`-style INI configs (`[program:x]` sections).
//!
//! Field vocabulary is largely shared (the target daemon's config was
//! designed with the same semantics: `autorestart`/`startsecs`/`stopwaitsecs`
//! aliases, `priority` startup order, per-stream log files), so mapping is
//! mostly 1:1. Anything that cannot be carried over becomes a structured
//! [`ImportWarning`] instead of a silent drop.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{ImportWarning, ParseCtx, StackDraft, StackFormat, WarningSeverity};
use crate::{AutorestartPolicy, CreateProgramRequest};

pub const FORMAT_ID: &str = "supervisor";

/// Sections that describe the source daemon's own machinery — skip silently.
const INFRA_SECTIONS: [&str; 3] = ["supervisord", "unix_http_server", "supervisorctl"];

/// A raw `key=value` line with its source line number (for errors).
#[derive(Debug, Clone)]
struct RawEntry {
    key: String,
    value: String,
}

/// One INI section with ordered entries.
#[derive(Debug, Default)]
struct RawSection {
    name: String,
    entries: Vec<RawEntry>,
}

pub struct SupervisorFormat;

impl StackFormat for SupervisorFormat {
    fn id(&self) -> &'static str {
        FORMAT_ID
    }

    fn detect(&self, input: &str) -> bool {
        input.lines().any(|l| {
            let t = l.trim();
            t.starts_with("[program:") || t == "[supervisord]"
        })
    }

    fn parse(&self, input: &str, ctx: &ParseCtx) -> anyhow::Result<StackDraft> {
        let mut sections = parse_ini(input)?;

        // Expand `[include] files=...` breadth-first, resolving relative
        // patterns against the including file's directory. Missing includes
        // are a hard error: a partial import would silently drop programs.
        let mut queue: Vec<(String, PathBuf)> = Vec::new();
        for sec in &sections {
            if sec.name == "include" {
                for e in &sec.entries {
                    if e.key == "files" {
                        let base = include_base(ctx)?;
                        queue.extend(
                            split_values(&e.value)
                                .into_iter()
                                .map(|p| (p, base.clone())),
                        );
                    }
                }
            }
        }
        let mut seen_includes: Vec<PathBuf> = Vec::new();
        let mut included_sections: Vec<RawSection> = Vec::new();
        while let Some((pat, base)) = queue.pop() {
            for file in expand_include_pattern(&pat, &base)? {
                if seen_includes.contains(&file) {
                    continue; // include cycle / duplicate — skip
                }
                seen_includes.push(file.clone());
                let content = std::fs::read_to_string(&file)
                    .map_err(|e| anyhow::anyhow!("[include] {}: {e}", file.display()))?;
                for sec in parse_ini(&content)? {
                    if sec.name == "include" {
                        let dir = file.parent().unwrap_or(Path::new(".")).to_path_buf();
                        for e in &sec.entries {
                            if e.key == "files" {
                                queue.extend(
                                    split_values(&e.value).into_iter().map(|p| (p, dir.clone())),
                                );
                            }
                        }
                    } else {
                        included_sections.push(sec);
                    }
                }
            }
        }
        sections.extend(included_sections);

        let mut draft = StackDraft::default();
        let mut env_cache: Option<HashMap<String, String>> = None;
        for sec in &sections {
            match classify(&sec.name) {
                SectionKind::Program => {
                    let env = env_cache
                        .get_or_insert_with(|| std::env::vars().collect())
                        .clone();
                    let prog_name = sec.name.strip_prefix("program:").unwrap_or(&sec.name);
                    map_program(prog_name, sec, ctx, &env, &mut draft)?;
                }
                SectionKind::Group => map_group(sec, &mut draft),
                SectionKind::Infra => {} // daemon machinery — skip silently
                SectionKind::Unknown => draft.warnings.push(ImportWarning {
                    severity: WarningSeverity::Warn,
                    section: format!("[{}]", sec.name),
                    message: "unknown section type; not imported".into(),
                    suggestion: Some(
                        "supported sections: [program:*], [group:*], [include]".to_string(),
                    ),
                }),
            }
        }

        if draft.services.is_empty() {
            anyhow::bail!("no [program:*] sections found — nothing to import");
        }
        Ok(draft)
    }
}

fn include_base(ctx: &ParseCtx) -> anyhow::Result<PathBuf> {
    ctx.here_dir
        .clone()
        .ok_or_else(|| anyhow::anyhow!("[include] requires a source file context (--file path)"))
}

/// `*.conf`-style include patterns: absolute/relative paths with `*`/`?`.
fn expand_include_pattern(pat: &str, base: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let path = if Path::new(pat).is_absolute() {
        PathBuf::from(pat)
    } else {
        base.join(pat)
    };
    let s = path.to_string_lossy();
    if !s.contains('*') && !s.contains('?') {
        return Ok(vec![path]);
    }
    let mut out: Vec<PathBuf> = glob::glob(&s)
        .map_err(|e| anyhow::anyhow!("[include] bad pattern {pat:?}: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| anyhow::anyhow!("[include] glob {pat:?}: {e}"))?;
    out.sort();
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SectionKind {
    Program,
    Group,
    Infra,
    Unknown,
}

fn classify(name: &str) -> SectionKind {
    if let Some(p) = name.strip_prefix("program:") {
        return if p.is_empty() {
            SectionKind::Unknown
        } else {
            SectionKind::Program
        };
    }
    if let Some(g) = name.strip_prefix("group:") {
        return if g.is_empty() {
            SectionKind::Unknown
        } else {
            SectionKind::Group
        };
    }
    let is_infra = INFRA_SECTIONS.contains(&name)
        || INFRA_SECTIONS
            .iter()
            .any(|p| name.strip_prefix(p).is_some_and(|r| r.starts_with(':')))
        // supervisor's rpcinterface factory section
        || name.starts_with("rpcinterface:");
    if is_infra {
        SectionKind::Infra
    } else {
        SectionKind::Unknown
    }
}

// ------------------------- INI lexing -------------------------

/// Minimal INI parser: `[section]`, `key=value`, `;`/`#` comments,
/// backslash-newline continuation. Values keep inner quotes (stripped later
/// where semantics demand, e.g. `environment`).
fn parse_ini(input: &str) -> anyhow::Result<Vec<RawSection>> {
    let mut sections: Vec<RawSection> = Vec::new();
    // Split into logical lines first, joining backslash continuations.
    let mut logical: Vec<(usize, String)> = Vec::new();
    let mut lines = input.lines().enumerate();
    while let Some((i, raw)) = lines.next() {
        let line_no = i + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
            continue;
        }
        let mut logical_line = trimmed.to_string();
        while logical_line.ends_with('\\') {
            logical_line.pop(); // drop the continuation backslash
            match lines.next() {
                Some((_, next)) => logical_line.push_str(next.trim()),
                None => break, // trailing backslash at EOF — keep as-is
            }
        }
        logical.push((line_no, logical_line));
    }

    for (line_no, line) in logical {
        if line.starts_with('[') {
            let end = line
                .find(']')
                .ok_or_else(|| anyhow::anyhow!("line {line_no}: unterminated section header"))?;
            let name = line[1..end].trim().to_string();
            if name.is_empty() {
                anyhow::bail!("line {line_no}: empty section name");
            }
            sections.push(RawSection {
                name,
                entries: Vec::new(),
            });
            continue;
        }
        let Some(sec) = sections.last_mut() else {
            anyhow::bail!("line {line_no}: key=value outside any [section]");
        };
        let Some((key, value)) = line.split_once('=') else {
            anyhow::bail!("line {line_no}: expected key=value");
        };
        let key = key.trim();
        if key.is_empty() {
            anyhow::bail!("line {line_no}: empty key");
        }
        sec.entries.push(RawEntry {
            key: key.to_string(),
            value: value.trim().to_string(),
        });
    }
    Ok(sections)
}

// ------------------------- value helpers -------------------------

/// Values separated by commas and/or whitespace (for `programs=a,b`).
fn split_values(s: &str) -> Vec<String> {
    s.split([',', ' ', '\t'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

/// Shell-like word split for command lines.
fn shlex_split(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut has_cur = false;
    let mut in_quote: Option<char> = None;
    for c in s.chars() {
        match in_quote {
            Some(q) if c == q => in_quote = None,
            Some(_) => {
                cur.push(c);
            }
            None if c == '"' || c == '\'' => {
                in_quote = Some(c);
                has_cur = true;
            }
            None if c.is_whitespace() => {
                if has_cur {
                    out.push(std::mem::take(&mut cur));
                    has_cur = false;
                }
            }
            None => {
                cur.push(c);
                has_cur = true;
            }
        }
    }
    if has_cur {
        out.push(cur);
    }
    out
}

/// Expand source placeholders in a value.
///
/// `%(ENV_X)s` resolves from `env` (the CLI's current shell environment);
/// `%(here)s` resolves to the source file's directory; `%(program_name)s` /
/// `%(group_name)s` resolve from context. Returns the expanded string plus
/// whether any `%(ENV_…)` placeholder was used (callers warn: the expansion
/// came from *this* shell, which may differ from the source daemon's).
fn expand_placeholders(
    value: &str,
    env: &HashMap<String, String>,
    here: Option<&Path>,
    program_name: &str,
) -> (String, bool) {
    let mut out = value.to_string();
    let mut used_env = false;
    for (token, rep) in [
        (
            "%(here)s".to_string(),
            here.map(|p| p.display().to_string()).unwrap_or_default(),
        ),
        ("%(program_name)s".to_string(), program_name.to_string()),
    ] {
        if out.contains(&token) {
            out = out.replace(&token, &rep);
        }
    }
    while let Some(start) = out.find("%(ENV_") {
        let Some(rest) = out.get(start..) else { break };
        let Some(close) = rest.find(")s") else { break };
        let Some(var) = rest.get(6..close) else { break };
        let token = format!("%(ENV_{var})s");
        let val = std::env::var(var).unwrap_or_default();
        let _ = env;
        used_env = true;
        out = out.replace(&token, &val);
    }
    (out, used_env)
}

/// `%(process_num)02d`-style tokens → `{num}` (printf width is not
/// representable in the target template; padding is dropped).
fn translate_process_num_template(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' && chars.get(i + 1) == Some(&'(') {
            let rest: String = chars[i + 2..].iter().collect();
            if let Some(close_rel) = rest.find(')') {
                let inner = &rest[..close_rel];
                if inner.starts_with("process_num") {
                    // Token: "%(" + inner + ")" + flags/width + conv char,
                    // e.g. `%(process_num)02d`. Only digits/printf flags may
                    // appear between ")" and the d/s conversion char.
                    let after = &rest[close_rel + 1..];
                    let width_len = after
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | ' '))
                        .count();
                    let conv = after.chars().nth(width_len);
                    if matches!(conv, Some('d') | Some('s')) {
                        out.push_str("{num}");
                        // "%(" + inner + ")" + width + conv
                        i += 2 + close_rel + 1 + width_len + 1;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// supervisord `environment=K="v",K2="v2"` — tolerant parser.
fn parse_environment(value: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let chars: Vec<char> = value.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let mut key = String::new();
        while i < chars.len() && chars[i] != '=' {
            if !chars[i].is_whitespace() && chars[i] != ',' {
                key.push(chars[i]);
            }
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        i += 1; // '='
        let mut val = String::new();
        if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
            let q = chars[i];
            i += 1;
            while i < chars.len() && chars[i] != q {
                val.push(chars[i]);
                i += 1;
            }
            i += 1; // closing quote
        } else {
            while i < chars.len() && chars[i] != ',' {
                val.push(chars[i]);
                i += 1;
            }
            val = val.trim().to_string();
        }
        while i < chars.len() && chars[i] == ',' {
            i += 1;
        }
        if !key.is_empty() {
            out.push((key, val));
        }
    }
    out
}

// ------------------------- mapping -------------------------

fn warn(sec_name: &str, message: impl Into<String>, suggestion: Option<String>) -> ImportWarning {
    ImportWarning {
        severity: WarningSeverity::Warn,
        section: format!("[program:{sec_name}]"),
        message: message.into(),
        suggestion,
    }
}

fn map_program(
    prog_name: &str,
    sec: &RawSection,
    ctx: &ParseCtx,
    shell_env: &HashMap<String, String>,
    draft: &mut StackDraft,
) -> anyhow::Result<()> {
    let name = prog_name.to_string();
    let entry = |key: &str| sec.entries.iter().find(|e| e.key == key);
    let expand = |v: &str| expand_placeholders(v, shell_env, ctx.here_dir.as_deref(), &name);

    let mut req = CreateProgramRequest {
        name: Some(name.clone()),
        // serde field defaults apply only when *deserializing*; the struct's
        // derived Default carries type zero values. Fill them explicitly so
        // emitted TOML/JSON is valid on the wire.
        retry_limit: 3,
        startsecs: 10,
        exitcodes: vec![0],
        numprocs: 1,
        ..Default::default()
    };

    // `command` may carry trailing args ("cmd --flag val").
    if let Some(e) = entry("command") {
        let (expanded, used_env) = expand(&e.value);
        if used_env {
            draft.warnings.push(warn(
                &name,
                "command contains %(ENV_…): expanded from the importing shell — verify the value",
                None,
            ));
        }
        let parts = shlex_split(expanded.trim());
        match parts.split_first() {
            Some((cmd, args)) => {
                req.command = cmd.clone();
                req.args = args.to_vec();
            }
            None => {
                return Err(anyhow::anyhow!(
                    "[program:{name}]: `command` is empty — cannot import without it"
                ));
            }
        }
    } else {
        return Err(anyhow::anyhow!(
            "[program:{name}]: missing `command` — cannot import without it"
        ));
    }

    if let Some(e) = entry("directory") {
        let (expanded, used_env) = expand(&e.value);
        if used_env {
            draft.warnings.push(warn(
                &name,
                "directory contains %(ENV_…): expanded from the importing shell — verify",
                None,
            ));
        }
        req.cwd = Some(expanded);
    }
    if let Some(e) = entry("user") {
        req.user = Some(e.value.clone());
    }
    if let Some(e) = entry("autostart") {
        req.autostart = matches!(e.value.trim(), "true" | "1" | "yes");
    }
    if let Some(e) = entry("autorestart") {
        match e.value.trim() {
            "true" | "1" | "yes" => req.autorestart = AutorestartPolicy::True,
            "false" | "0" | "no" => req.autorestart = AutorestartPolicy::False,
            "unexpected" => req.autorestart = AutorestartPolicy::Unexpected,
            other => draft.warnings.push(warn(
                &name,
                format!("autorestart={other:?}: unknown value; using default (unexpected)"),
                None,
            )),
        }
    }
    if let Some(e) = entry("exitcodes") {
        let parsed: Vec<i32> = e
            .value
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if parsed.is_empty() {
            draft.warnings.push(warn(
                &name,
                format!(
                    "exitcodes={:?}: no integers parsed; keeping default [0]",
                    e.value
                ),
                None,
            ));
        } else {
            req.exitcodes = parsed;
        }
    }
    if let Some(e) = entry("startsecs") {
        match e.value.trim().parse() {
            Ok(v) => req.startsecs = v,
            Err(_) => draft.warnings.push(warn(
                &name,
                format!("startsecs={:?}: not a number; ignored", e.value),
                None,
            )),
        }
    }
    // `stopwaitsecs` is an alias of the target's `stopsecs`.
    if let Some(e) = entry("stopwaitsecs") {
        match e.value.trim().parse() {
            Ok(v) => req.stopsecs = Some(v),
            Err(_) => draft.warnings.push(warn(
                &name,
                format!("stopwaitsecs={:?}: not a number; ignored", e.value),
                None,
            )),
        }
    }
    if let Some(e) = entry("stopsignal") {
        let sig = e.value.trim().to_uppercase();
        if !sig.is_empty() && sig != "TERM" {
            draft.warnings.push(warn(
                &name,
                format!("stopsignal={sig}: stop always sends SIGTERM here; send {sig} manually via `super signal {name} {}`", sig.to_lowercase()),
                None,
            ));
        }
    }
    if let Some(e) = entry("startretries") {
        if let Ok(v) = e.value.trim().parse() {
            req.retry_limit = v;
        }
        draft.warnings.push(warn(
            &name,
            "startretries mapped to retry_limit (general restart cap; start-phase retry semantics differ slightly)",
            None,
        ));
    }
    if let Some(e) = entry("priority") {
        match e.value.trim().parse() {
            Ok(v) => req.priority = v,
            Err(_) => draft.warnings.push(warn(
                &name,
                format!("priority={:?}: not an integer; ignored", e.value),
                None,
            )),
        }
    }
    if let Some(e) = entry("environment") {
        for (k, v) in parse_environment(&e.value) {
            if v.contains("%(ENV_") {
                draft.warnings.push(warn(
                    &name,
                    format!(
                        "environment {k} uses %(ENV_…): expanded from the importing shell — verify"
                    ),
                    None,
                ));
                let (expanded, _) = expand(&v);
                req.env.insert(k, expanded);
            } else if v.contains("%(") {
                let (expanded, _) = expand(&v);
                req.env.insert(k, expanded);
            } else {
                req.env.insert(k, v);
            }
        }
    }

    // Logs. Custom log paths are security-confined to the target log dir.
    // With `--remap-logs`, foreign absolute paths become bare file names so
    // they land inside the spool; without it they are kept as-is and the
    // apply-side validator rejects anything outside the log dir.
    let map_log = |raw: &str, stream: &str, warnings: &mut Vec<ImportWarning>| -> Option<String> {
        let raw = raw.trim();
        if raw.is_empty() || raw.eq_ignore_ascii_case("none") || raw == "/dev/null" {
            return None;
        }
        if ctx.remap_logs {
            let fname = Path::new(raw)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("{name}-{stream}.log"));
            if fname != raw {
                warnings.push(warn(
                    &name,
                    format!("{stream} log {raw:?} remapped to {fname:?} (log paths must live in the target log dir)"),
                    None,
                ));
            }
            Some(fname)
        } else {
            Some(raw.to_string())
        }
    };
    if let Some(e) = entry("stdout_logfile") {
        req.stdout_logfile = map_log(&e.value, "stdout", &mut draft.warnings);
    }
    if let Some(e) = entry("stderr_logfile") {
        req.stderr_logfile = map_log(&e.value, "stderr", &mut draft.warnings);
    }
    // redirect_stderr=true → single merged stream: point stderr at stdout.
    if let Some(e) = entry("redirect_stderr")
        && matches!(e.value.trim(), "true" | "1" | "yes")
        && let Some(out) = req.stdout_logfile.clone()
    {
        req.stderr_logfile = Some(out);
    }
    // Rotation-related keys collapse into ONE warning per section (they all
    // point at the same global [child_logging] answer; one per key floods
    // the report).
    let rotation_keys = [
        "stdout_logfile_maxbytes",
        "stdout_logfile_backups",
        "stderr_logfile_maxbytes",
        "stderr_logfile_backups",
        "stdout_logfile_datefmt",
        "stderr_logfile_datefmt",
    ];
    let rotation_hits: Vec<String> = rotation_keys
        .iter()
        .filter_map(|k| entry(k).map(|e| format!("{k}={}", e.value)))
        .collect();
    if !rotation_hits.is_empty() {
        draft.warnings.push(warn(
            &name,
            format!(
                "log rotation keys ({}): rotation is global here ([child_logging] in conf/super.toml), not per program",
                rotation_hits.join(", ")
            ),
            Some("tune [child_logging] max_size_mb / max_backups".to_string()),
        ));
    }

    // numprocs + process_name.
    let numprocs: u32 = entry("numprocs")
        .and_then(|e| e.value.trim().parse().ok())
        .unwrap_or(1);
    if numprocs > 1 {
        let pname = entry("process_name").map(|e| e.value.trim());
        let template_has_num = pname.is_some_and(|p| p.contains("%(process_num"));
        if !template_has_num {
            draft.warnings.push(warn(
                &name,
                format!("numprocs={numprocs} but process_name lacks %(process_num): the source config would create duplicate names; suffixes -0..-{} are applied here", numprocs - 1),
                None,
            ));
        }
        req.numprocs = numprocs;
        if let Some(p) = pname {
            let translated = translate_process_num_template(
                &p.replace("%(program_name)s", "{name}")
                    .replace("%(group_name)s", "{name}"),
            );
            if translated.contains("%(") {
                draft.warnings.push(warn(
                    &name,
                    format!("process_name={p:?}: unsupported placeholder; default naming used"),
                    None,
                ));
            } else {
                req.process_name = Some(translated);
            }
        }
    }

    // Dropped-with-message set. Group termination is the default behavior on
    // the target (the child runs in its own process group), so those two keys
    // need no warning at all.
    for key in ["umask", "autorestarts", "serverurl"] {
        if entry(key).is_some() {
            draft
                .warnings
                .push(warn(&name, format!("{key}: no equivalent; dropped"), None));
        }
    }

    draft.services.push(req);
    Ok(())
}

/// Map `[group:x] programs=a,b` onto each named program's `group` field.
fn map_group(sec: &RawSection, draft: &mut StackDraft) {
    let gname = match sec.name.strip_prefix("group:") {
        Some(g) if !g.is_empty() => g.to_string(),
        _ => return,
    };
    let members: Vec<String> = sec
        .entries
        .iter()
        .find(|e| e.key == "programs")
        .map(|e| split_values(&e.value))
        .unwrap_or_default();
    if members.is_empty() {
        draft.warnings.push(ImportWarning {
            severity: WarningSeverity::Warn,
            section: format!("[group:{gname}]"),
            message: "no `programs=` list found; group ignored".into(),
            suggestion: None,
        });
        return;
    }
    for svc in &mut draft.services {
        if let Some(n) = &svc.name
            && members.iter().any(|m| m == n)
        {
            svc.group = Some(gname.clone());
        }
    }
}

// ------------------------- tests -------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> StackDraft {
        SupervisorFormat
            .parse(input, &ParseCtx::default())
            .expect("parse ok")
    }

    #[test]
    fn detect_rejects_toml_and_accepts_ini() {
        assert!(!SupervisorFormat.detect("[[services]]\nname = \"web\"\n"));
        assert!(SupervisorFormat.detect("[program:web]\ncommand=/bin/x\n"));
        assert!(SupervisorFormat.detect("[supervisord]\nnodaemon=true\n"));
    }

    #[test]
    fn basic_program_maps_fields() {
        let d = parse(
            "[program:web]\n\
             command=/usr/bin/node server.js --port 8080\n\
             directory=/srv/web\n\
             user=www-data\n\
             autostart=true\n\
             autorestart=unexpected\n\
             startsecs=5\n\
             stopwaitsecs=30\n\
             priority=100\n\
             environment=PORT=\"8080\",DEBUG=\"1\"\n",
        );
        assert_eq!(d.services.len(), 1);
        let s = &d.services[0];
        assert_eq!(s.name.as_deref(), Some("web"));
        assert_eq!(s.command, "/usr/bin/node");
        assert_eq!(s.args, vec!["server.js", "--port", "8080"]);
        assert_eq!(s.cwd.as_deref(), Some("/srv/web"));
        assert_eq!(s.user.as_deref(), Some("www-data"));
        assert!(s.autostart);
        assert_eq!(s.autorestart, AutorestartPolicy::Unexpected);
        assert_eq!(s.startsecs, 5);
        assert_eq!(s.stopsecs, Some(30));
        assert_eq!(s.priority, 100);
        assert_eq!(s.env.get("PORT").map(String::as_str), Some("8080"));
        assert_eq!(s.env.get("DEBUG").map(String::as_str), Some("1"));
        assert!(d.warnings.is_empty());
    }

    #[test]
    fn missing_command_is_error() {
        let err = SupervisorFormat
            .parse("[program:web]\nuser=x\n", &ParseCtx::default())
            .unwrap_err();
        assert!(err.to_string().contains("missing `command`"));
    }

    #[test]
    fn infra_sections_skipped_silently() {
        let d = parse(
            "[supervisord]\n\
             nodaemon=true\n\
             logfile=/var/log/supervisord.log\n\
             \n\
             [program:web]\n\
             command=/bin/sleep 30\n\
             \n\
             [unix_http_server]\n\
             file=/tmp/s.sock\n\
             \n\
             [supervisorctl]\n\
             serverurl=unix:///tmp/s.sock\n\
             \n\
             [rpcinterface:supervisor]\n\
             supervisor.rpcinterface_factory=supervisor.rpcinterface:make_main_rpcinterface\n",
        );
        assert_eq!(d.services.len(), 1);
        assert!(d.warnings.is_empty());
    }

    #[test]
    fn unknown_section_warns() {
        let d = parse(
            "[program:web]\ncommand=/bin/sleep 30\n\n[eventlistener:mailer]\ncommand=/bin/mail\n",
        );
        assert_eq!(d.services.len(), 1);
        assert!(
            d.warnings
                .iter()
                .any(|w| w.section.contains("eventlistener"))
        );
    }

    #[test]
    fn autorestart_true_false_unexpected() {
        for (src, want) in [
            ("false", AutorestartPolicy::False),
            ("0", AutorestartPolicy::False),
            ("true", AutorestartPolicy::True),
            ("unexpected", AutorestartPolicy::Unexpected),
        ] {
            let d = parse(&format!(
                "[program:a]\ncommand=/bin/sleep 1\nautorestart={src}\n"
            ));
            assert_eq!(d.services[0].autorestart, want, "autorestart={src}");
        }
    }

    #[test]
    fn exitcodes_csv_parses() {
        let d = parse("[program:a]\ncommand=/bin/sleep 1\nexitcodes=0,2\n");
        assert_eq!(d.services[0].exitcodes, vec![0, 2]);
    }

    #[test]
    fn numprocs_with_template() {
        let d = parse(
            "[program:worker]\n\
             command=/bin/worker\n\
             numprocs=4\n\
             process_name=%(program_name)s_%(process_num)02d\n",
        );
        let s = &d.services[0];
        assert_eq!(s.numprocs, 4);
        assert_eq!(s.process_name.as_deref(), Some("{name}_{num}"));
        assert!(d.warnings.is_empty());
    }

    #[test]
    fn redirect_stderr_merges_streams() {
        let d = parse(
            "[program:a]\ncommand=/bin/sleep 1\nredirect_stderr=true\nstdout_logfile=/var/log/a.log\n",
        );
        let s = &d.services[0];
        assert_eq!(s.stdout_logfile, s.stderr_logfile);
    }

    #[test]
    fn log_rotation_keys_collapse_to_one_warning() {
        let d = parse(
            "[program:a]\n\
             command=/bin/sleep 1\n\
             stdout_logfile_maxbytes=50MB\n\
             stdout_logfile_backups=10\n\
             stdout_logfile_datefmt=%%Y-%%m-%%d\n",
        );
        assert_eq!(d.warnings.len(), 1);
        assert!(d.warnings[0].message.contains("rotation is global"));
        assert!(
            d.warnings[0]
                .message
                .contains("stdout_logfile_maxbytes=50MB")
        );
        assert!(d.warnings[0].message.contains("stdout_logfile_datefmt"));
    }

    #[test]
    fn stopasgroup_needs_no_warning() {
        let d = parse("[program:a]\ncommand=/bin/sleep 1\nstopasgroup=true\nkillasgroup=true\n");
        assert!(d.warnings.is_empty());
    }

    #[test]
    fn comments_and_continuations() {
        let d = parse(
            "# top comment\n\
             ; also comment\n\
             [program:a]\n\
             ; inner comment\n\
             command=/bin/echo a b \\\n  c d\n",
        );
        let s = &d.services[0];
        assert_eq!(s.command, "/bin/echo");
        assert_eq!(s.args, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn env_placeholder_warns_and_expands() {
        // SAFETY(test): single-threaded test binary section; setting a unique
        // test var is not read elsewhere.
        unsafe { std::env::set_var("IMPORT_TEST_VAR", "hello") };
        let d = parse(
            "[program:a]\ncommand=/bin/sleep 1\nenvironment=GREET=\"%(ENV_IMPORT_TEST_VAR)s world\"\n",
        );
        assert_eq!(
            d.services[0].env.get("GREET").map(String::as_str),
            Some("hello world")
        );
        assert!(d.warnings.iter().any(|w| w.message.contains("%(ENV_")));
    }

    #[test]
    fn group_maps_members() {
        let d = parse(
            "[program:web]\ncommand=/bin/sleep 1\n\n[program:db]\ncommand=/bin/sleep 2\n\n[group:app]\nprograms=web,db\n",
        );
        assert_eq!(d.services.len(), 2);
        assert!(d.services.iter().all(|s| s.group.as_deref() == Some("app")));
    }

    #[test]
    fn umask_and_friends_warn() {
        let d = parse("[program:a]\ncommand=/bin/sleep 1\numask=022\nserverurl=unix:///x\n");
        assert!(d.warnings.iter().any(|w| w.message.contains("umask")));
        assert!(d.warnings.iter().any(|w| w.message.contains("serverurl")));
    }

    #[test]
    fn stopsignal_non_term_warns_with_hint() {
        let d = parse("[program:a]\ncommand=/bin/sleep 1\nstopsignal=QUIT\n");
        let w = d
            .warnings
            .iter()
            .find(|w| w.message.contains("stopsignal=QUIT"))
            .expect("stopsignal warning");
        assert!(w.message.contains("super signal a quit"));
    }

    #[test]
    fn quoted_command_args_survive() {
        let d = parse("[program:a]\ncommand=/bin/echo \"hello world\" 'x y'\n");
        let s = &d.services[0];
        assert_eq!(s.args, vec!["hello world", "x y"]);
    }

    #[test]
    fn bare_command_without_args() {
        let d = parse("[program:a]\ncommand=/usr/sbin/nginx\n");
        assert_eq!(d.services[0].command, "/usr/sbin/nginx");
        assert!(d.services[0].args.is_empty());
    }

    #[test]
    fn include_expands_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(root.join("a.conf"), "[program:a]\ncommand=/bin/sleep 1\n").unwrap();
        std::fs::write(root.join("b.conf"), "[program:b]\ncommand=/bin/sleep 2\n").unwrap();
        let main = format!(
            "[include]\nfiles={}/a.conf {}/b.conf\n",
            root.display(),
            root.display()
        );
        let d = SupervisorFormat
            .parse(
                &main,
                &ParseCtx {
                    here_dir: Some(root.to_path_buf()),
                    allow_include: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(d.services.len(), 2);
    }

    #[test]
    fn include_glob_expands() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(root.join("w1.conf"), "[program:w1]\ncommand=/bin/sleep 1\n").unwrap();
        std::fs::write(root.join("w2.conf"), "[program:w2]\ncommand=/bin/sleep 2\n").unwrap();
        let main = format!("[include]\nfiles={}/w*.conf\n", root.display());
        let d = SupervisorFormat
            .parse(
                &main,
                &ParseCtx {
                    here_dir: Some(root.to_path_buf()),
                    allow_include: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(d.services.len(), 2);
    }

    #[test]
    fn missing_include_is_hard_error() {
        let err = SupervisorFormat
            .parse(
                "[include]\nfiles=/definitely/not/here/x.conf\n",
                &ParseCtx {
                    here_dir: Some(PathBuf::from("/tmp")),
                    allow_include: true,
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert!(err.to_string().contains("[include]"));
    }
}
