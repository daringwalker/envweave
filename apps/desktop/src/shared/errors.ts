/** Converts Tauri command rejections and ordinary JavaScript errors into readable UI text. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === "object") {
    if ("message" in error && typeof error.message === "string") return error.message;
    if ("error" in error) return errorMessage(error.error);
    try {
      const serialized = JSON.stringify(error);
      if (serialized && serialized !== "{}") return serialized;
    } catch {
      // Fall through to a stable message for non-serializable values.
    }
  }
  return "操作失败，未返回可读的错误信息";
}
