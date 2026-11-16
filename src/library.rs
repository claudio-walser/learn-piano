//! Reads `compositions.xml` and discovers difficulty levels and lessons on disk.

use std::path::{Path, PathBuf};

const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// A text available in several languages (`xml:lang`).
#[derive(Clone, Debug, Default)]
pub struct Localized(pub Vec<(String, String)>);

impl Localized {
    pub fn get(&self, lang: &str) -> &str {
        self.0
            .iter()
            .find(|(l, _)| l == lang)
            .or_else(|| self.0.iter().find(|(l, _)| l == "en"))
            .or_else(|| self.0.first())
            .map(|(_, t)| t.as_str())
            .unwrap_or("")
    }
}

pub struct DifficultyDef {
    pub id: String,
    pub title: Localized,
    /// `kind="duet"`: not a difficulty step but a duet arrangement.
    pub duet: bool,
}

pub struct Lesson {
    pub path: PathBuf,
    /// File name without extension, e.g. `01-part-A` or `full`.
    pub stem: String,
    /// Short label of a part lesson, e.g. `A`.
    pub label: String,
    pub is_full: bool,
}

pub struct Level {
    pub difficulty: String,
    /// Index into [`Library::difficulties`] (used for colors and titles).
    pub difficulty_index: usize,
    pub is_duet: bool,
    /// Part lessons in order, followed by `full.musicxml` as last lesson.
    pub lessons: Vec<Lesson>,
}

pub struct Composition {
    pub id: String,
    pub title: Localized,
    pub composer: Option<String>,
    pub thumbnail: Option<PathBuf>,
    #[allow(dead_code)]
    pub tags: Vec<String>,
    pub levels: Vec<Level>,
}

pub struct Library {
    pub root: PathBuf,
    pub difficulties: Vec<DifficultyDef>,
    pub compositions: Vec<Composition>,
}

/// Finds the folder containing `compositions.xml`:
/// `--library <dir>`, `$LEARN_PIANO_LIBRARY`, the working directory,
/// or the executable's folder and its parents.
pub fn find_root() -> Option<PathBuf> {
    let has_lib = |p: &Path| p.join("compositions.xml").is_file();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--library") {
        if let Some(p) = args.get(i + 1) {
            return Some(PathBuf::from(p));
        }
    }
    if let Ok(p) = std::env::var("LEARN_PIANO_LIBRARY") {
        return Some(PathBuf::from(p));
    }
    if let Ok(cwd) = std::env::current_dir() {
        if has_lib(&cwd) {
            return Some(cwd);
        }
    }
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().skip(1).find(|p| has_lib(p)).map(Path::to_path_buf)
}

fn localized(node: roxmltree::Node, tag: &str) -> Localized {
    Localized(
        node.children()
            .filter(|c| c.has_tag_name(tag))
            .map(|c| {
                let lang = c.attribute((XML_NS, "lang")).unwrap_or("en").to_string();
                (lang, c.text().unwrap_or("").trim().to_string())
            })
            .collect(),
    )
}

impl Library {
    pub fn load(root: &Path) -> Result<Library, String> {
        let file = root.join("compositions.xml");
        let text = std::fs::read_to_string(&file)
            .map_err(|e| format!("{}: {e}", file.display()))?;
        let doc = roxmltree::Document::parse(&text)
            .map_err(|e| format!("{}: {e}", file.display()))?;
        let top = doc.root_element();

        let difficulties: Vec<DifficultyDef> = top
            .descendants()
            .filter(|n| n.has_tag_name("difficulty"))
            .filter_map(|n| {
                Some(DifficultyDef {
                    id: n.attribute("id")?.to_string(),
                    title: localized(n, "title"),
                    duet: n.attribute("kind") == Some("duet"),
                })
            })
            .collect();

        // Paths are relative to the `compositions/` folder (or to the XML file if it has none).
        let base = Some(root.join("compositions")).filter(|p| p.is_dir()).unwrap_or_else(|| root.to_path_buf());
        let mut compositions = Vec::new();
        for n in top.descendants().filter(|n| n.has_tag_name("composition")) {
            let Some(id) = n.attribute("id") else { continue };
            let dir = base.join(n.attribute("path").unwrap_or(id));
            let thumbnail = n
                .children()
                .find(|c| c.has_tag_name("thumbnail"))
                .and_then(|t| t.attribute("src"))
                .map(|src| base.join(src));
            let composer = n
                .children()
                .find(|c| c.has_tag_name("composer"))
                .and_then(|c| c.text())
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty());
            let tags = n
                .descendants()
                .filter(|c| c.has_tag_name("tag"))
                .filter_map(|c| c.text().map(|t| t.trim().to_string()))
                .collect();
            let levels = difficulties
                .iter()
                .enumerate()
                .filter_map(|(i, d)| scan_level(&dir.join(&d.id), &d.id, i, d.duet))
                .collect();
            compositions.push(Composition {
                id: id.to_string(),
                title: localized(n, "title"),
                composer,
                thumbnail,
                tags,
                levels,
            });
        }

        Ok(Library { root: root.to_path_buf(), difficulties, compositions })
    }

    pub fn difficulty_title(&self, level: &Level, lang: &str) -> String {
        self.difficulties
            .get(level.difficulty_index)
            .map(|d| d.title.get(lang).to_string())
            .unwrap_or_else(|| level.difficulty.clone())
    }
}

/// A level exists when `<dir>/full.musicxml` exists. Files named
/// `<digits>-part*.musicxml` next to it are the lessons, in name order.
fn scan_level(dir: &Path, id: &str, index: usize, is_duet: bool) -> Option<Level> {
    let full = dir.join("full.musicxml");
    if !full.is_file() {
        return None;
    }
    let mut parts: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let stem = name.strip_suffix(".musicxml")?;
            let (num, rest) = stem.split_once('-')?;
            (num.chars().all(|c| c.is_ascii_digit()) && rest.starts_with("part"))
                .then(|| (stem.to_string(), e.path()))
        })
        .collect();
    parts.sort();

    let mut lessons: Vec<Lesson> = parts
        .into_iter()
        .map(|(stem, path)| {
            let rest = stem.split_once('-').map(|x| x.1).unwrap_or(&stem);
            let label = rest
                .trim_start_matches("part")
                .trim_start_matches(['-', '_'])
                .replace(['-', '_'], " ");
            Lesson { path, label, stem, is_full: false }
        })
        .collect();
    lessons.push(Lesson { path: full, stem: "full".into(), label: String::new(), is_full: true });

    Some(Level { difficulty: id.to_string(), difficulty_index: index, is_duet, lessons })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_bundled_library() {
        let lib = Library::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert_eq!(lib.difficulties.len(), 5);
        assert!(lib.difficulties.iter().any(|d| d.id == "duet" && d.duet));
        let c = &lib.compositions[0];
        assert_eq!(c.composer.as_deref(), Some("Ernst Anschütz"));
        assert!(c.thumbnail.as_ref().unwrap().is_file(), "{:?}", c.thumbnail);
        assert_eq!(c.levels.len(), 1);
        let stems: Vec<&str> = c.levels[0].lessons.iter().map(|l| l.stem.as_str()).collect();
        assert_eq!(stems, ["01-part-A", "02-part-B", "full"]);
        assert_eq!(c.levels[0].lessons[0].label, "A");

        let sn = lib.compositions.iter().find(|c| c.id == "silent-night").unwrap();
        let ids: Vec<&str> = sn.levels.iter().map(|l| l.difficulty.as_str()).collect();
        assert_eq!(ids, ["prep", "beginner", "intermediate"]);
        assert!(sn.levels.iter().all(|l| l.lessons.len() == 4));
    }
}
