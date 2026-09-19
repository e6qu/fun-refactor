import type { NextResponse } from "next/server";

export const __order: string[] = [];

export async function mark(
  _request: Request,
  next: () => Promise<NextResponse>,
): Promise<NextResponse> {
  __order.push("mark");
  return next();
}

export async function audit(
  _request: Request,
  next: () => Promise<NextResponse>,
): Promise<NextResponse> {
  __order.push("audit");
  return next();
}
