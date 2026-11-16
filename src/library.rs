//! Reads `compositions.xml` and discovers difficulty levels and lessons on disk.
//!
//! The library's texts (titles of pieces, difficulty levels and categories) live in
//! `<library>/locales/<lang>/compositions.ftl` as `composition-<id>`, `difficulty-<id>` and
//! `tag-<id>`. They are read at runtime, so a library can bring its own translations.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use i18n_embed::fluent::FluentLanguageLoader;
use i18n_embed::{I18nAssets, LanguageLoader};
use unic_langid::LanguageIdentifier;

const DOMAIN: &str = "compositions";

/// `<library>/locales/<lang>/compositions.ftl` as Fluent assets.
struct LibraryAssets(PathBuf);

impl I18nAssets for LibraryAssets {
    fn get_files(&self, file_path: &str) -> Vec<Cow<'_, [u8]>> {
        std::fs::read(self.0.join(file_path)).map(Cow::from).into_iter().collect()
    }

    /// `<lang>/compositions.ftl` for every language folder.
    fn filenames_iter(&self) -> Box<dyn Iterator<Item = String> + '_> {
        let file = format!("{DOMAIN}.ftl");
        Box::new(subfolders(&self.0).into_iter().filter(move |(_, p)| p.join(&file).is_file()).map(|(lang, _)| format!("{lang}/{DOMAIN}.ftl")))
    }
}

/// Languages of a library and the messages per language that are missing compared to
/// English or that English does not have.
pub struct TranslationReport {
    pub lang: String,
    pub missing: Vec<String>,
    pub extra: Vec<String>,
}

pub struct DifficultyDef {
    pub id: String,
    /// In the current language (see [`Library::localize`]).
    pub title: String,
    /// `kind="duet"`: not a difficulty step but a duet arrangement.
    pub duet: bool,
}

/// A category (`<tags><tag id="…">` in compositions.xml).
pub struct TagDef {
    pub id: String,
    /// In the current language (see [`Library::localize`]).
    pub title: String,
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
    /// In the current language, and in English for the search (see [`Library::localize`]).
    pub title: String,
    pub title_en: String,
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
    /// All languages of `locales/`; `None` when the library has no texts at all.
    texts: Option<FluentLanguageLoader>,
}

/// Fallback text for an id without translation: "silent-night" → "Silent night".
fn prettify(id: &str) -> String {
    let t = id.replace(['-', '_'], " ");
    let mut c = t.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
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

/// Loads every language in `<root>/locales`. English is the fallback.
fn load_texts(root: &Path) -> Option<FluentLanguageLoader> {
    let assets = LibraryAssets(root.join("locales"));
    let loader = FluentLanguageLoader::new(DOMAIN, "en".parse().unwrap());
    let langs = loader.available_languages(&assets).ok().filter(|l| !l.is_empty())?;
    if let Err(e) = loader.load_languages(&assets, &langs) {
        eprintln!("{}: {e}", root.join("locales").display());
        return None;
    }
    loader.set_use_isolating(false);
    Some(loader)
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
                    title: String::new(),
                    duet: n.attribute("kind") == Some("duet"),
                })
            })
            .collect();

        let tags: Vec<TagDef> = top
            .children()
            .filter(|n| n.has_tag_name("tags"))
            .flat_map(|n| n.children().filter(|c| c.has_tag_name("tag")))
            .filter_map(|n| Some(TagDef { id: n.attribute("id")?.to_string(), title: String::new() }))
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
                title: String::new(),
                title_en: String::new(),
                composer,
                thumbnail,
                tags,
                levels,
            });
        }

        let texts = load_texts(root);
        let mut lib = Library { root: root.to_path_buf(), difficulties, tags, compositions, texts };
        lib.localize(&"en".parse().unwrap());
        Ok(lib)
    }

    /// Sets all titles to `lang`, falling back to English and then to the id.
    pub fn localize(&mut self, lang: &LanguageIdentifier) {
        let en: LanguageIdentifier = "en".parse().unwrap();
        let pick = |l: &LanguageIdentifier| self.texts.as_ref().map(|t| t.select_languages(&[l]));
        let (cur, eng) = (pick(lang), pick(&en));
        let get = |loader: &Option<FluentLanguageLoader>, prefix: &str, id: &str| {
            let key = format!("{prefix}-{id}");
            match loader {
                Some(l) if l.has(&key) => l.get(&key),
                _ => prettify(id),
            }
        };
        for d in &mut self.difficulties {
            d.title = get(&cur, "difficulty", &d.id);
        }
        for t in &mut self.tags {
            t.title = get(&cur, "tag", &t.id);
        }
        for c in &mut self.compositions {
            c.title = get(&cur, "composition", &c.id);
            c.title_en = get(&eng, "composition", &c.id);
        }
    }

    /// Message ids every language should have: one per composition, difficulty and
    /// category (also categories only used by compositions).
    fn expected_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.difficulties.iter().map(|d| format!("difficulty-{}", d.id)).collect();
        ids.extend(self.categories().iter().chain(self.tags.iter().map(|t| &t.id)).map(|t| format!("tag-{t}")));
        ids.extend(self.compositions.iter().map(|c| format!("composition-{}", c.id)));
        ids.sort();
        ids.dedup();
        ids
    }

    /// For `--check-library`: what is missing or left over per language.
    pub fn check_translations(&self) -> Vec<TranslationReport> {
        let expected = self.expected_ids();
        let Some(texts) = &self.texts else {
            return vec![TranslationReport { lang: "en".into(), missing: expected, extra: Vec::new() }];
        };
        let mut langs = texts.available_languages(&LibraryAssets(self.root.join("locales"))).unwrap_or_default();
        langs.sort_by_key(|l| (l.to_string() != "en", l.to_string()));
        langs
            .iter()
            .map(|lang| {
                let have: Vec<String> = texts.with_message_iter(lang, |it| it.map(|m| m.id.name.to_string()).collect());
                TranslationReport {
                    lang: lang.to_string(),
                    missing: expected.iter().filter(|id| !have.contains(id)).cloned().collect(),
                    extra: have.iter().filter(|id| !expected.contains(id)).cloned().collect(),
                }
            })
            .collect()
    }

    pub fn tag_title(&self, id: &str) -> String {
        match self.tags.iter().find(|t| t.id == id) {
            Some(t) => t.title.clone(),
            None => prettify(id),
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
    /// (title in the current language or English, composer or category), sorted by title.
    pub fn visible(&self, tag: Option<&str>, query: &str) -> Vec<usize> {
        let words: Vec<String> = fold(query).split_whitespace().map(str::to_string).collect();
        let mut v: Vec<(String, usize)> = self
            .compositions
            .iter()
            .enumerate()
            .filter(|(_, c)| tag.is_none_or(|t| c.tags.iter().any(|x| x == t)))
            .filter(|(_, c)| {
                let mut hay: Vec<String> = vec![c.title.clone(), c.title_en.clone()];
                hay.extend(c.composer.clone());
                hay.extend(c.tags.iter().flat_map(|id| [id.clone(), self.tag_title(id)]));
                let hay = fold(&hay.join(" "));
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .map(|(i, c)| (fold(&c.title), i))
            .collect();
        v.sort();
        v.into_iter().map(|(_, i)| i).collect()
    }

    pub fn difficulty_title(&self, level: &Level) -> String {
        self.difficulties
            .get(level.difficulty_index)
            .map(|d| d.title.clone())
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
        let mut lib = Library::load(&root).unwrap();
        assert_eq!(lib.difficulties.len(), 5);
        // Every language of the bundled library is complete.
        for r in lib.check_translations() {
            assert!(r.missing.is_empty() && r.extra.is_empty(), "{}: missing {:?}, extra {:?}", r.lang, r.missing, r.extra);
        }
        assert_eq!(lib.compositions[1].title, "Silent Night");
        lib.localize(&"de".parse().unwrap());
        assert_eq!(lib.compositions[1].title, "Stille Nacht");
        assert_eq!(lib.compositions[1].title_en, "Silent Night");
        assert_eq!(lib.difficulty_title(&lib.compositions[1].levels[0]), "Vorstufe");
        assert!(lib.difficulties.iter().any(|d| d.id == "duet" && d.duet));
        let c = &lib.compositions[0];
        assert_eq!(c.composer.as_deref(), Some("Ernst Anschütz"));
        assert!(c.thumbnail.as_ref().unwrap().is_file(), "{:?}", c.thumbnail);
        assert_eq!(c.levels.len(), 1);
        let stems: Vec<&str> = c.levels[0].lessons.iter().map(|l| l.stem.as_str()).collect();
        assert_eq!(stems, ["01-part-A", "02-part-B", "full"]);
        assert_eq!(c.levels[0].lessons[0].label, "A");

        // Categories, alphabetical order, search and filter.
        assert_eq!(lib.tag_title("christmas"), "Weihnachten");
        assert_eq!(lib.tag_title("rock-pop"), "Rock pop");
        assert!(lib.categories().contains(&"christmas".to_string()));
        let ids = |v: Vec<usize>| v.into_iter().map(|i| lib.compositions[i].id.as_str()).collect::<Vec<_>>();
        let all = ids(lib.visible(None, ""));
        assert_eq!(all[..3], ["all-my-little-ducklings", "die-zauberfloete", "fuer-elise"]);
        assert_eq!(ids(lib.visible(Some("christmas"), "")), ["oh-christmas-tree", "silent-night"]);
        assert_eq!(ids(lib.visible(None, "fur")), ["fuer-elise"]);
        assert_eq!(ids(lib.visible(None, "beethoven ode")), ["ode-to-joy"]);
        assert_eq!(ids(lib.visible(None, "weihnacht")), ["oh-christmas-tree", "silent-night"]);
        // English titles are found in German too.
        assert_eq!(ids(lib.visible(None, "silent")), ["silent-night"]);
        assert!(lib.visible(Some("birthday"), "elise").is_empty());

        let sn = lib.compositions.iter().find(|c| c.id == "silent-night").unwrap();
        let ids: Vec<&str> = sn.levels.iter().map(|l| l.difficulty.as_str()).collect();
        assert_eq!(ids, ["prep", "beginner", "intermediate"]);
        assert!(sn.levels.iter().all(|l| l.lessons.len() == 4));
    }
}
