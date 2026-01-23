use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn main() {
    let dist_dir = Path::new("web/dist");

    // Ensure `cargo run` picks up frontend rebuilds (Vite output is embedded via `include_dir!`).
    println!("cargo:rerun-if-changed=web/dist");

    let mut hasher = DefaultHasher::new();
    if dist_dir.exists() {
        hash_dir(dist_dir, &mut hasher);
    } else {
        "missing-web-dist".hash(&mut hasher);
    }
    let id = format!("{:x}", hasher.finish());
    println!("cargo:rustc-env=CATNAP_WEB_DIST_BUILD_ID={id}");
}

fn hash_dir(dir: &Path, hasher: &mut DefaultHasher) {
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        println!("cargo:rerun-if-changed={}", path.display());

        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            println!("cargo:rerun-if-changed={}", p.display());

            p.display().to_string().hash(hasher);
            if let Ok(meta) = entry.metadata() {
                meta.len().hash(hasher);
                hash_modified(meta.modified().ok(), hasher);
                if meta.is_dir() {
                    stack.push(p);
                }
            }
        }
    }
}

fn hash_modified(m: Option<SystemTime>, hasher: &mut DefaultHasher) {
    let Some(m) = m else {
        0u64.hash(hasher);
        return;
    };
    let Ok(dur) = m.duration_since(UNIX_EPOCH) else {
        0u64.hash(hasher);
        return;
    };
    dur.as_secs().hash(hasher);
    dur.subsec_nanos().hash(hasher);
}
