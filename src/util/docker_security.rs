/*!
Docker SecurityOptions parsing helper.

Parses the JSON-ish array returned by:
  docker info --format "{{json .SecurityOptions}}"

Extracts:
- has_apparmor: true if any item mentions "apparmor"
- seccomp_profile: value of "profile=" within the "name=seccomp" item, or "(unknown)"
- cgroupns_mode: value of "mode=" within the "name=cgroupns" item, or "(unknown)"
- rootless: true if any item mentions "rootless"

Also exposes the raw string items to allow callers to render a pretty list.
*/

#[derive(Debug, Clone)]
pub struct DockerSecurityOptions {
    pub items: Vec<String>,
    pub has_apparmor: bool,
    pub seccomp_profile: String,
    pub cgroupns_mode: String,
    pub rootless: bool,
}

/// Parse Docker SecurityOptions JSON-ish output into a structured summary and raw items.
pub fn docker_security_options_parse(raw_json_str: &str) -> DockerSecurityOptions {
    // Extract quoted items from a JSON string array without external deps
    let mut items: Vec<String> = Vec::new();
    let mut in_str = false;
    let mut esc = false;
    let mut buf = String::new();
    for ch in raw_json_str.chars() {
        if in_str {
            if esc {
                buf.push(ch);
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                items.push(buf.clone());
                buf.clear();
                in_str = false;
            } else {
                buf.push(ch);
            }
        } else if ch == '"' {
            in_str = true;
        }
    }

    // Defaults
    let mut has_apparmor = false;
    let mut seccomp = String::from("(unknown)");
    let mut cgroupns = String::from("(unknown)");
    let mut rootless = false;

    for s in &items {
        let sl = s.to_ascii_lowercase();
        if sl.contains("apparmor") {
            has_apparmor = true;
        }
        if sl.contains("rootless") {
            rootless = true;
        }
        if sl.contains("name=seccomp") {
            for part in s.split(',') {
                if let Some(v) = part.strip_prefix("profile=") {
                    seccomp = v.to_string();
                    break;
                }
            }
        }
        if sl.contains("name=cgroupns") {
            for part in s.split(',') {
                if let Some(v) = part.strip_prefix("mode=") {
                    cgroupns = v.to_string();
                    break;
                }
            }
        }
    }

    DockerSecurityOptions {
        items,
        has_apparmor,
        seccomp_profile: seccomp,
        cgroupns_mode: cgroupns,
        rootless,
    }
}
