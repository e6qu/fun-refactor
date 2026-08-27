const std = @import("std");

fn frPrint(comptime format: []const u8, args: anytype) void {
    var buffer: [1024]u8 = undefined;
    var writer = std.Io.File.stdout().writerStreaming(std.Options.debug_io, &buffer);
    writer.interface.print(format, args) catch unreachable;
    writer.interface.flush() catch unreachable;
}

fn floorDiv(a: i64, b: i64) i64 {
    const quotient = @divTrunc(a, b);
    if (@rem(a, b) != 0 and (a < 0) != (b < 0)) {
        return quotient - 1;
    }
    return quotient;
}

fn floorMod(a: i64, b: i64) i64 {
    return a - floorDiv(a, b) * b;
}

pub fn main() void {
    frPrint("start\n", .{});
    const a: i64 = 7;
    const b: i64 = 2;
    frPrint("sum {d}\n", .{a + b});
    frPrint("diff {d}\n", .{a - b});
    frPrint("product {d}\n", .{a * b});
    frPrint("quotient {d}\n", .{floorDiv(a, b)});
    frPrint("remainder {d}\n", .{floorMod(a, b)});
    const negative: i64 = -7;
    frPrint("negquotient {d}\n", .{floorDiv(negative, b)});
    frPrint("negremainder {d}\n", .{floorMod(negative, b)});
    frPrint("done\n", .{});
}
