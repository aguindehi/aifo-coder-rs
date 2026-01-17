#![allow(clippy::module_name_repetitions)]

use std::ffi::OsString;
use std::path::Path;

fn os_vec(parts: &[&str]) -> Vec<OsString> {
    parts.iter().map(OsString::from).collect()
}

#[test]
fn unit_node_outside_workspace_goes_local() {
    let argv = os_vec(&["node", "/usr/local/lib/node_modules/somepkg/bin.js"]);
    let program = aifo_coder::shim::node_main_program_arg(&argv).expect("program");
    let p = aifo_coder::shim::resolve_program_path_with_cwd(&program, Path::new("/workspace"));
    assert!(
        !aifo_coder::shim::is_under_workspace(&p),
        "expected outside /workspace: {}",
        p.display()
    );
}

#[test]
fn unit_node_inside_workspace_goes_proxy() {
    let argv = os_vec(&["node", "scripts/run.js"]);
    let program = aifo_coder::shim::node_main_program_arg(&argv).expect("program");
    let p = aifo_coder::shim::resolve_program_path_with_cwd(&program, Path::new("/workspace"));
    assert!(
        aifo_coder::shim::is_under_workspace(&p),
        "expected under /workspace: {}",
        p.display()
    );
}

#[test]
fn unit_python_module_mode_goes_local() {
    let argv = os_vec(&["python3", "-m", "pip"]);
    assert!(aifo_coder::shim::python_is_module_mode(&argv));
}

#[test]
fn unit_python_script_under_workspace_goes_proxy() {
    let argv = os_vec(&["python3", "tools/test.py"]);
    assert!(!aifo_coder::shim::python_is_module_mode(&argv));
    let script = aifo_coder::shim::python_script_arg(&argv).expect("script");
    let p = aifo_coder::shim::resolve_program_path_with_cwd(&script, Path::new("/workspace"));
    assert!(aifo_coder::shim::is_under_workspace(&p));
}

#[test]
fn unit_python_script_outside_workspace_goes_local() {
    let argv = os_vec(&["python3", "/etc/run.py"]);
    let inv = aifo_coder::shim::python_invocation(&argv, Some(Path::new("/workspace/app")));
    assert_eq!(inv.kind, aifo_coder::shim::PythonKind::Script);
    assert!(inv.target.is_some());
    assert!(!inv.target_in_workspace());
}

#[test]
fn unit_python_dash_c_uses_cwd_for_routing() {
    let argv = os_vec(&["python3", "-c", "print('hi')"]);
    let inv = aifo_coder::shim::python_invocation(&argv, Some(Path::new("/workspace")));
    assert_eq!(inv.kind, aifo_coder::shim::PythonKind::Command);
    assert!(inv.target_in_workspace());
}

#[test]
fn unit_python_stdin_uses_cwd_for_routing() {
    let argv = os_vec(&["python3", "-"]);
    let inv = aifo_coder::shim::python_invocation(&argv, Some(Path::new("/tmp")));
    assert_eq!(inv.kind, aifo_coder::shim::PythonKind::Stdin);
    assert!(!inv.target_in_workspace());
}

#[test]
fn unit_python_module_pip_forces_toolchain() {
    let argv = os_vec(&["python3", "-m", "pip", "install", "foo"]);
    let inv = aifo_coder::shim::python_invocation(&argv, Some(Path::new("/workspace")));
    assert_eq!(inv.kind, aifo_coder::shim::PythonKind::Module);
    assert!(inv.target_in_workspace());
    assert!(inv.module_is_pip_like());
}

#[test]
fn unit_python_module_pip_minor_forces_toolchain() {
    let argv = os_vec(&["python3", "-m", "pip3.11", "install", "foo"]);
    let inv = aifo_coder::shim::python_invocation(&argv, Some(Path::new("/workspace")));
    assert_eq!(inv.kind, aifo_coder::shim::PythonKind::Module);
    assert!(inv.module_is_pip_like());
}

#[test]
fn unit_pip_and_uv_are_always_proxy_by_policy() {
    for tool in ["pip", "pip3", "pip3.11", "uv"] {
        assert!(aifo_coder::shim::tool_is_always_proxy(tool));
    }
    assert!(!aifo_coder::shim::tool_is_always_proxy("uvx"));
}

#[test]
fn unit_uvx_from_flag_is_detected() {
    let argv = os_vec(&[
        "uvx",
        "--from",
        "git+https://github.com/example/tool",
        "tool",
    ]);
    assert!(aifo_coder::shim::uvx_has_from_flag(&argv));
}

#[test]
fn unit_uvx_from_equals_flag_is_detected() {
    let argv = os_vec(&["uvx", "--from=git+https://github.com/example/tool", "tool"]);
    assert!(aifo_coder::shim::uvx_has_from_flag(&argv));
}

#[test]
fn unit_uvx_from_flag_ignored_after_separator() {
    let argv = os_vec(&["uvx", "tool", "--", "--from", "git+https://ignored"]);
    assert!(!aifo_coder::shim::uvx_has_from_flag(&argv));
}
