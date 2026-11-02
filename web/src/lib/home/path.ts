import { loadCourse, type CourseModule, type LessonSummary } from "../lesson/content";
import type { ProgressStore } from "../progress/store";

export type LessonState = "studied" | "open" | "locked";
/** `n` is the lesson's 1-based place in its module. */
export type PathLesson = LessonSummary & { n: number; state: LessonState };
/** `next` is the first lesson not studied yet, or null once the module is done. */
export type PathModule = Omit<CourseModule, "lessons"> & { lessons: PathLesson[]; studied: number; next: PathLesson | null };
export type Path = { title: string; modules: PathModule[] };

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

export async function loadPath(progress: ProgressStore): Promise<Path> {
  const [course, studied] = await Promise.all([loadCourse(), progress.studied()]);
  return { title: course.title, modules: pathState(course.modules, studied) };
}
