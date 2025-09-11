pub fn contains_env(preview: &str, key: &str) -> bool {
    preview.contains(&format!("-e {}=", key))
        || preview.contains(&format!("-e '{}=", key))
        || preview.contains(&format!("-e \"{}=", key))
        || preview.contains(&format!(" {}=", key)) // tolerate raw KEY= forms in some previews
}

pub fn assert_preview_path_includes(preview: &str, components: &[&str]) {
    assert!(
        contains_env(preview, "PATH") || preview.contains(" PATH="),
        "PATH not exported in preview: {}",
        preview
    );
    for c in components {
        assert!(
            preview.contains(c),
            "missing PATH component '{}'; preview: {}",
            c,
            preview
        );
    }
}

#[allow(dead_code)]
pub fn assert_preview_path_has_any(preview: &str, components: &[&str]) {
    assert!(
        contains_env(preview, "PATH") || preview.contains(" PATH="),
        "PATH not exported in preview: {}",
        preview
    );
    let ok = components.iter().any(|c| preview.contains(c));
    assert!(
        ok,
        "PATH lacks any of {:?}; preview: {}",
        components, preview
    );
}
