export class NextResponse extends Response {
  static next() {
    return new NextResponse(null, { status: 200 });
  }
}
