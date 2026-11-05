import { loadCatalog, type Course, type CourseModule, type LessonSummary } from "../lesson/content";
import type { ProgressStore } from "../progress/store";

export type LessonState = "studied" | "open" | "locked";
/** `n` is the lesson's 1-based place in its module. */
export type PathLesson = LessonSummary & { n: number; state: LessonState };
/** `next` is the first lesson not studied yet, or null once the module is done. */
export type PathModule = Omit<CourseModule, "lessons"> & { lessons: PathLesson[]; studied: number; next: PathLesson | null };
export type PathCourse = Omit<Course, "modules"> & { modules: PathModule[] };
export type Path = { courses: PathCourse[] };

/** Lesson i of a module is open when i is 0 or lesson i-1 is studied. Studied lessons stay open. */
export function pathState(modules: CourseModule[], studied: Record<string, string>): PathModule[] {
  return modules.map((module) => {
    const lessons = module.lessons.map(
      (lesson, i): PathLesson => ({
        ...lesson,
        n: i + 1,
        state: studied[lesson.id] ? "studied" : i === 0 || studied[module.lessons[i - 1].id] ? "open" : "locked",
      }),
    );
    return {
      ...module,
      lessons,
      studied: lessons.filter((l) => l.state === "studied").length,
      next: lessons.find((l) => l.state !== "studied") ?? null,
    };
  });
}

/** The first module, across courses in order, that `match` accepts; `index` is its place within its course. */
export function findModule<C extends { modules: { id: string }[] }>(
  courses: C[],
  match: (module: C["modules"][number]) => boolean,
): { course: C; module: C["modules"][number]; index: number } | null {
  for (const course of courses) {
    const index = course.modules.findIndex(match);
    if (index >= 0) return { course, module: course.modules[index], index };
  }
  return null;
}

export async function loadPath(progress: ProgressStore): Promise<Path> {
  const [catalog, studied] = await Promise.all([loadCatalog(), progress.studied()]);
  return { courses: catalog.courses.map((course) => ({ ...course, modules: pathState(course.modules, studied) })) };
}
