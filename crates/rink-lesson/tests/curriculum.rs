//! Every lesson under `<repo>/curriculum/` parses, passes `check`, and the
//! catalog in `courses.json` lists each one exactly once.

use std::fs;
use std::path::{Path, PathBuf};

use rink_core::io::from_json;
use rink_lesson::{check, Catalog, Lesson, Plays};

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn load_play(dir: &Path, stem: &str) -> rink_core::Play {
    let path = dir.join(format!("{stem}.rink.json"));
    from_json(&read(&path)).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn every_lesson_checks_clean_and_is_in_the_course() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
    let catalog_path = root.join("courses.json");
    if !catalog_path.exists() {
        eprintln!("no {}, nothing to check", catalog_path.display());
        return;
    }
    let catalog = Catalog::parse(&read(&catalog_path)).unwrap_or_else(|e| panic!("courses.json: {e}"));

    let mut md_files: Vec<PathBuf> = fs::read_dir(root.join("lessons"))
        .expect("curriculum/lessons/")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    md_files.sort();

    let mut lessons = Vec::new();
    let mut failures = Vec::new();
    for md_path in &md_files {
        let steps_path = md_path.with_extension("steps.json");
        let lesson = match Lesson::parse(&read(md_path), &read(&steps_path)) {
            Ok(lesson) => lesson,
            Err(e) => {
                failures.push(format!("{}: {e}", md_path.display()));
                continue;
            }
        };
        let plays_dir = root.join("plays");
        let plays = Plays {
            main: load_play(&plays_dir, &lesson.meta.play),
            wrong: lesson.meta.wrong.as_deref().map(|stem| load_play(&plays_dir, stem)),
        };
        for issue in check(&lesson, &plays) {
            failures.push(format!("{} {}: {}", lesson.meta.id, issue.step.unwrap_or_default(), issue.message));
        }
        lessons.push(lesson);
    }
    failures.extend(catalog.check(&lessons).into_iter().map(|i| format!("courses.json: {}", i.message)));
    assert!(failures.is_empty(), "{} lesson(s):\n{}", lessons.len(), failures.join("\n"));
    eprintln!("{} lessons checked", lessons.len());
}
