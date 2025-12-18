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
fn unit_pip_and_uv_are_always_proxy_by_policy() {
    for tool in ["pip", "pip3", "uv", "uvx"] {
        assert!(aifo_coder::shim::tool_is_always_proxy(tool));
    }
}
