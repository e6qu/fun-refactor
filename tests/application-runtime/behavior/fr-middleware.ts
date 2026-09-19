import type { NextFunction, Request, Response } from "express";

export function mark(_req: Request, res: Response, next: NextFunction): void {
  res.append("x-fr-order", "mark");
  next();
}

export function audit(_req: Request, res: Response, next: NextFunction): void {
  res.append("x-fr-order", "audit");
  next();
}
