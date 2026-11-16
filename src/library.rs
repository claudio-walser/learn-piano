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

/// A category (`<tags><tag id="…">` in compositions.xml).
pub struct TagDef {
    pub id: String,
    pub title: Localized,
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
    pub tags: Vec<String>,
    pub levels: Vec<Level>,
}

pub struct Library {
    pub root: PathBuf,
    pub difficulties: Vec<DifficultyDef>,
    pub tags: Vec<TagDef>,
    pub compositions: Vec<Composition>,
}

/// Lower case without accents, for search and sorting ("Für" matches "fur").
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        match c {
            'ä' | 'à' | 'á' | 'â' | 'å' => out.push('a'),
            'ö' | 'ò' | 'ó' | 'ô' => out.push('o'),
            'ü' | 'ù' | 'ú' | 'û' => out.push('u'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'ï' | 'î' | 'í' | 'ì' => out.push('i'),
            'ç' => out.push('c'),
            'ñ' => out.push('n'),
            'ß' => out.push_str("ss"),
            c => out.push(c),
        }
    }
    out
}

/// The library folder for `dir`: `dir` itself when it contains `compositions.xml`,
/// or its `compositions/` subfolder when that one does (e.g. the project folder).
pub fn resolve(dir: &Path) -> Option<PathBuf> {
    [dir.to_path_buf(), dir.join("compositions")]
        .into_iter()
        .find(|d| d.join("compositions.xml").is_file())
}

/// Finds the library folder: `--library <dir>`, `$LEARN_PIANO_LIBRARY`, the folder chosen
/// in the settings, then the working directory and the executable's folder and its parents.
/// Returns the path that was tried when an explicit choice does not contain a library.
pub fn find_root(chosen: Option<&Path>) -> Result<PathBuf, Option<PathBuf>> {
    let args: Vec<String> = std::env::args().collect();
    let explicit = args
        .iter()
        .position(|a| a == "--library")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LEARN_PIANO_LIBRARY").map(PathBuf::from))
        .or_else(|| chosen.map(Path::to_path_buf));
    if let Some(dir) = explicit {
        return resolve(&dir).ok_or(Some(dir));
    }
    search_dirs().iter().find_map(|d| resolve(d)).ok_or(None)
}

/// The working directory, then the executable's folder and its parents.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::current_dir().into_iter().collect();
    if let Ok(exe) = std::env::current_exe() {
        dirs.extend(exe.ancestors().skip(1).map(Path::to_path_buf));
    }
    dirs
}

/// `sound-fonts/default.sf2` next to the library, in its parent folder, in the working
/// directory or next to the executable (or one of its parents).
pub fn find_soundfont(library: Option<&Path>) -> Option<PathBuf> {
    let near_lib = library.into_iter().flat_map(|l| [Some(l.to_path_buf()), l.parent().map(Path::to_path_buf)]).flatten();
    near_lib
        .chain(search_dirs())
        .map(|d| d.join("sound-fonts").join("default.sf2"))
        .find(|p| p.is_file())
}

/// Visible subfolders of `dir`, sorted by name (for the folder picker).
pub fn subfolders(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut v: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
                .filter(|(n, _)| !n.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|(n, _)| n.to_lowercase());
    v
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

        let tags: Vec<TagDef> = top
            .children()
            .filter(|n| n.has_tag_name("tags"))
            .flat_map(|n| n.children().filter(|c| c.has_tag_name("tag")))
            .filter_map(|n| Some(TagDef { id: n.attribute("id")?.to_string(), title: localized(n, "title") }))
            .collect();

        // Paths are relative to the folder of compositions.xml.
        let base = root;
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

        Ok(Library { root: root.to_path_buf(), difficulties, tags, compositions })
    }

    pub fn tag_title(&self, id: &str, lang: &str) -> String {
        match self.tags.iter().find(|t| t.id == id) {
            Some(t) => t.title.get(lang).to_string(),
            None => {
                let mut c = id.chars();
                c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
            }
        }
    }

    /// Tags used by at least one composition: defined ones in definition order, then the rest by name.
    pub fn categories(&self) -> Vec<String> {
        let used = |id: &str| self.compositions.iter().any(|c| c.tags.iter().any(|t| t == id));
        let mut out: Vec<String> = self.tags.iter().map(|t| t.id.clone()).filter(|id| used(id)).collect();
        let mut rest: Vec<String> = self
            .compositions
            .iter()
            .flat_map(|c| c.tags.iter().cloned())
            .filter(|t| !self.tags.iter().any(|d| &d.id == t))
            .collect();
        rest.sort();
        rest.dedup();
        out.extend(rest);
        out
    }

    /// Indices of the compositions with tag `tag` (all for `None`) matching `query`
    /// (in any title language, the composer or a category name), sorted by title.
    pub fn visible(&self, tag: Option<&str>, query: &str, lang: &str) -> Vec<usize> {
        let words: Vec<String> = fold(query).split_whitespace().map(str::to_string).collect();
        let mut v: Vec<(String, usize)> = self
            .compositions
            .iter()
            .enumerate()
            .filter(|(_, c)| tag.is_none_or(|t| c.tags.iter().any(|x| x == t)))
            .filter(|(_, c)| {
                let mut hay: Vec<&str> = c.title.0.iter().map(|(_, t)| t.as_str()).collect();
                hay.extend(c.composer.as_deref());
                let tag_names: Vec<String> = c
                    .tags
                    .iter()
                    .flat_map(|id| {
                        let def = self.tags.iter().find(|d| &d.id == id);
                        std::iter::once(id.clone()).chain(def.into_iter().flat_map(|d| d.title.0.iter().map(|(_, t)| t.clone())))
                    })
                    .collect();
                hay.extend(tag_names.iter().map(String::as_str));
                let hay = fold(&hay.join(" "));
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .map(|(i, c)| (fold(c.title.get(lang)), i))
            .collect();
        v.sort();
        v.into_iter().map(|(_, i)| i).collect()
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
        let project = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = resolve(project).unwrap();
        assert_eq!(root, project.join("compositions"));
        assert_eq!(resolve(&root).unwrap(), root);
        assert!(resolve(&project.join("src")).is_none());
        assert!(find_soundfont(Some(&root)).is_some());
        let lib = Library::load(&root).unwrap();
        assert_eq!(lib.difficulties.len(), 5);
        assert!(lib.difficulties.iter().any(|d| d.id == "duet" && d.duet));
        let c = &lib.compositions[0];
        assert_eq!(c.composer.as_deref(), Some("Ernst Anschütz"));
        assert!(c.thumbnail.as_ref().unwrap().is_file(), "{:?}", c.thumbnail);
        assert_eq!(c.levels.len(), 1);
        let stems: Vec<&str> = c.levels[0].lessons.iter().map(|l| l.stem.as_str()).collect();
        assert_eq!(stems, ["01-part-A", "02-part-B", "full"]);
        assert_eq!(c.levels[0].lessons[0].label, "A");

        // Categories, alphabetical order, search and filter.
        assert_eq!(lib.tag_title("christmas", "de"), "Weihnachten");
        assert_eq!(lib.tag_title("unknown", "de"), "Unknown");
        assert!(lib.categories().contains(&"christmas".to_string()));
        let ids = |v: Vec<usize>| v.into_iter().map(|i| lib.compositions[i].id.as_str()).collect::<Vec<_>>();
        let all = ids(lib.visible(None, "", "de"));
        assert_eq!(all[..2], ["all-my-little-ducklings", "fuer-elise"]);
        assert_eq!(ids(lib.visible(Some("christmas"), "", "de")), ["oh-christmas-tree", "silent-night"]);
        assert_eq!(ids(lib.visible(None, "fur", "de")), ["fuer-elise"]);
        assert_eq!(ids(lib.visible(None, "beethoven ode", "de")), ["ode-to-joy"]);
        assert_eq!(ids(lib.visible(None, "weihnacht", "en")), ["oh-christmas-tree", "silent-night"]);
        assert!(lib.visible(Some("birthday"), "elise", "de").is_empty());

        let sn = lib.compositions.iter().find(|c| c.id == "silent-night").unwrap();
        let ids: Vec<&str> = sn.levels.iter().map(|l| l.difficulty.as_str()).collect();
        assert_eq!(ids, ["prep", "beginner", "intermediate"]);
        assert!(sn.levels.iter().all(|l| l.lessons.len() == 4));
    }
}
