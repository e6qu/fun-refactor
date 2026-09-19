export type NextRequest = Request;
export declare class NextResponse extends Response {
  static next(): NextResponse;
}
