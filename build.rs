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
    let mut queue: Vec<PathBuf> = vec![dir.to_path_buf()];
    let mut i = 0usize;
    while i < queue.len() {
        let path = &queue[i];
        i += 1;

        println!("cargo:rerun-if-changed={}", path.display());

        let Ok(entries) = fs::read_dir(path) else {
            continue;
        };

        // `read_dir` iteration order is not guaranteed; sort paths so the build id is stable
        // when the dist contents are unchanged.
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();

        for p in paths {
            println!("cargo:rerun-if-changed={}", p.display());

            p.to_string_lossy().hash(hasher);
            let Ok(meta) = fs::symlink_metadata(&p) else {
                continue;
            };

            if meta.is_dir() {
                queue.push(p);
                continue;
            }

            meta.len().hash(hasher);
            if meta.is_file() {
                if let Ok(bytes) = fs::read(&p) {
                    bytes.hash(hasher);
                } else {
                    hash_modified(meta.modified().ok(), hasher);
                }
            } else {
                hash_modified(meta.modified().ok(), hasher);
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
