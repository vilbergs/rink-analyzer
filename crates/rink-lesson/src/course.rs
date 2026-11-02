//! `course.json`: the modules in order, each with its lessons in order.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Issue, Lesson, LessonError, SCHEMA};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Course {
    pub schema: u32,
    #[serde(default)]
    pub title: String,
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

impl Course {
    pub fn parse(json: &str) -> Result<Course, LessonError> {
        let course: Course = serde_json::from_str(json)?;
        if course.schema != SCHEMA {
            return Err(LessonError::Schema(course.schema));
        }
        Ok(course)
    }

    /// Every listed lesson exists, sits in the module its frontmatter names,
    /// and every lesson is listed exactly once.
    pub fn check(&self, lessons: &[Lesson]) -> Vec<Issue> {
        let by_id: BTreeMap<&str, &Lesson> = lessons.iter().map(|l| (l.meta.id.as_str(), l)).collect();
        let mut listed: BTreeMap<&str, usize> = BTreeMap::new();
        let mut issues = Vec::new();
        let mut issue = |message: String| issues.push(Issue { step: None, message });
        for module in &self.modules {
            for id in &module.lessons {
                *listed.entry(id).or_default() += 1;
                match by_id.get(id.as_str()) {
                    None => issue(format!("module `{}` lists `{id}`, which is not a lesson", module.id)),
                    Some(l) if l.meta.module != module.id => {
                        issue(format!("`{id}` is listed in `{}` but declares module `{}`", module.id, l.meta.module))
                    }
                    Some(_) => {}
                }
            }
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

    fn course(modules: &str) -> Course {
        Course::parse(&format!(r#"{{ "schema": 1, "modules": {modules} }}"#)).unwrap()
    }

    fn messages(course: &Course, lessons: &[Lesson]) -> Vec<String> {
        course.check(lessons).into_iter().map(|i| i.message).collect()
    }

    #[test]
    fn a_complete_course_is_clean() {
        let c = course(r#"[ { "id": "dzone", "title": "D-zone", "lessons": ["l1", "l2"] } ]"#);
        assert_eq!(messages(&c, &[lesson("l1", "dzone"), lesson("l2", "dzone")]), Vec::<String>::new());
    }

    #[test]
    fn listed_lessons_exist_in_their_module() {
        let c = course(r#"[ { "id": "dzone", "title": "D-zone", "lessons": ["l1", "l9"] } ]"#);
        assert_eq!(messages(&c, &[lesson("l1", "dzone")]), ["module `dzone` lists `l9`, which is not a lesson"]);
        let c = course(r#"[ { "id": "dzone", "title": "D-zone", "lessons": ["l1"] } ]"#);
        assert_eq!(messages(&c, &[lesson("l1", "nzone")]), ["`l1` is listed in `dzone` but declares module `nzone`"]);
    }

    #[test]
    fn every_lesson_is_listed_once() {
        let c = course(r#"[ { "id": "dzone", "title": "D-zone", "lessons": ["l1", "l1"] } ]"#);
        assert_eq!(
            messages(&c, &[lesson("l1", "dzone"), lesson("l2", "dzone")]),
            ["`l1` is listed 2 times", "`l2` is not listed in any module"]
        );
    }

    #[test]
    fn rejects_other_schemas() {
        assert!(matches!(Course::parse(r#"{ "schema": 2, "modules": [] }"#), Err(LessonError::Schema(2))));
    }
}
