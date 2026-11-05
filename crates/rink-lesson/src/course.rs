//! `courses.json`: the courses in order, each with its modules in order, each
//! with its lessons in order. A course lists the courses it builds on, which
//! come before it; module ids are unique across the whole catalog.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{Issue, Lesson, LessonError, SCHEMA};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub schema: u32,
    pub courses: Vec<Course>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Course {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Ids of courses this one builds on. Informational: nothing locks.
    #[serde(default)]
    pub requires: Vec<String>,
    pub modules: Vec<Module>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Module {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Lesson ids, in path order.
    pub lessons: Vec<String>,
}

impl Catalog {
    pub fn parse(json: &str) -> Result<Catalog, LessonError> {
        let catalog: Catalog = serde_json::from_str(json)?;
        if catalog.schema != SCHEMA {
            return Err(LessonError::Schema(catalog.schema));
        }
        Ok(catalog)
    }

    /// Course ids are unique, each `requires` names an earlier course, module
    /// ids are unique across courses, every listed lesson exists and sits in
    /// the module its frontmatter names, and every lesson is listed exactly once.
    pub fn check(&self, lessons: &[Lesson]) -> Vec<Issue> {
        let by_id: BTreeMap<&str, &Lesson> = lessons.iter().map(|l| (l.meta.id.as_str(), l)).collect();
        let mut before: BTreeSet<&str> = BTreeSet::new();
        let mut module_ids: BTreeSet<&str> = BTreeSet::new();
        let mut listed: BTreeMap<&str, usize> = BTreeMap::new();
        let mut issues = Vec::new();
        let mut issue = |message: String| issues.push(Issue { step: None, message });
        for course in &self.courses {
            if before.contains(course.id.as_str()) {
                issue(format!("course `{}` appears twice", course.id));
            }
            for id in &course.requires {
                if !before.contains(id.as_str()) {
                    issue(format!("course `{}` requires `{id}`, which is not a course listed before it", course.id));
                }
            }
            for module in &course.modules {
                if !module_ids.insert(&module.id) {
                    issue(format!("module `{}` appears more than once", module.id));
                }
                for id in &module.lessons {
                    *listed.entry(id).or_default() += 1;
                    match by_id.get(id.as_str()) {
                        None => issue(format!("module `{}` lists `{id}`, which is not a lesson", module.id)),
                        Some(l) if l.meta.module != module.id => issue(format!(
                            "`{id}` is listed in `{}` but declares module `{}`",
                            module.id, l.meta.module
                        )),
                        Some(_) => {}
                    }
                }
            }
            before.insert(&course.id);
        }
        for lesson in lessons {
            match listed.get(lesson.meta.id.as_str()).copied().unwrap_or(0) {
                1 => {}
                0 => issue(format!("`{}` is not listed in any module", lesson.meta.id)),
                n => issue(format!("`{}` is listed {n} times", lesson.meta.id)),
            }
        }
        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lesson(id: &str, module: &str) -> Lesson {
        let md = format!("---\nid: {id}\nmodule: {module}\ntitle: t\nplay: p\n---\n");
        Lesson::parse(&md, &format!(r#"{{ "schema": 1, "lesson": "{id}", "steps": [] }}"#)).unwrap()
    }

    fn catalog(courses: &str) -> Catalog {
        Catalog::parse(&format!(r#"{{ "schema": 1, "courses": {courses} }}"#)).unwrap()
    }

    fn one_course(modules: &str) -> Catalog {
        catalog(&format!(r#"[ {{ "id": "winger", "title": "Winger", "modules": {modules} }} ]"#))
    }

    fn messages(catalog: &Catalog, lessons: &[Lesson]) -> Vec<String> {
        catalog.check(lessons).into_iter().map(|i| i.message).collect()
    }

    #[test]
    fn a_complete_catalog_is_clean() {
        let c = catalog(
            r#"[
                { "id": "fundamentals", "title": "F", "modules": [ { "id": "rink", "title": "Rink", "lessons": ["f1"] } ] },
                { "id": "winger", "title": "W", "requires": ["fundamentals"],
                  "modules": [ { "id": "dzone", "title": "D-zone", "lessons": ["l1", "l2"] } ] }
            ]"#,
        );
        let lessons = [lesson("f1", "rink"), lesson("l1", "dzone"), lesson("l2", "dzone")];
        assert_eq!(messages(&c, &lessons), Vec::<String>::new());
    }

    #[test]
    fn listed_lessons_exist_in_their_module() {
        let c = one_course(r#"[ { "id": "dzone", "title": "D-zone", "lessons": ["l1", "l9"] } ]"#);
        assert_eq!(messages(&c, &[lesson("l1", "dzone")]), ["module `dzone` lists `l9`, which is not a lesson"]);
        let c = one_course(r#"[ { "id": "dzone", "title": "D-zone", "lessons": ["l1"] } ]"#);
        assert_eq!(messages(&c, &[lesson("l1", "nzone")]), ["`l1` is listed in `dzone` but declares module `nzone`"]);
    }

    #[test]
    fn every_lesson_is_listed_once_across_courses() {
        let c = catalog(
            r#"[
                { "id": "a", "title": "A", "modules": [ { "id": "dzone", "title": "D", "lessons": ["l1"] } ] },
                { "id": "b", "title": "B", "modules": [ { "id": "nzone", "title": "N", "lessons": ["l1"] } ] }
            ]"#,
        );
        assert_eq!(
            messages(&c, &[lesson("l1", "dzone"), lesson("l2", "dzone")]),
            [
                "`l1` is listed in `nzone` but declares module `dzone`",
                "`l1` is listed 2 times",
                "`l2` is not listed in any module"
            ]
        );
    }

    #[test]
    fn course_ids_are_unique() {
        let c = catalog(
            r#"[
                { "id": "winger", "title": "A", "modules": [] },
                { "id": "winger", "title": "B", "modules": [] }
            ]"#,
        );
        assert_eq!(messages(&c, &[]), ["course `winger` appears twice"]);
    }

    #[test]
    fn requires_names_an_earlier_course() {
        let c = catalog(
            r#"[
                { "id": "winger", "title": "W", "requires": ["fundamentals", "goalie"], "modules": [] },
                { "id": "fundamentals", "title": "F", "modules": [] }
            ]"#,
        );
        assert_eq!(
            messages(&c, &[]),
            [
                "course `winger` requires `fundamentals`, which is not a course listed before it",
                "course `winger` requires `goalie`, which is not a course listed before it",
            ]
        );
    }

    #[test]
    fn module_ids_are_unique_across_courses() {
        let c = catalog(
            r#"[
                { "id": "a", "title": "A", "modules": [ { "id": "dzone", "title": "D", "lessons": [] } ] },
                { "id": "b", "title": "B", "modules": [ { "id": "dzone", "title": "D", "lessons": [] } ] }
            ]"#,
        );
        assert_eq!(messages(&c, &[]), ["module `dzone` appears more than once"]);
    }

    #[test]
    fn rejects_other_schemas() {
        assert!(matches!(Catalog::parse(r#"{ "schema": 2, "courses": [] }"#), Err(LessonError::Schema(2))));
    }
}
