const std = @import("std");

fn frPrint(comptime format: []const u8, args: anytype) void {
    var buffer: [1024]u8 = undefined;
    var writer = std.Io.File.stdout().writerStreaming(std.Options.debug_io, &buffer);
    writer.interface.print(format, args) catch unreachable;
    writer.interface.flush() catch unreachable;
}

fn applyTo(f: fn (i64) i64, n: i64) i64 {
    return f(n);
}

fn twice(f: fn (i64) i64, n: i64) i64 {
    return f(f(n));
}

fn addOne(n: i64) i64 {
    return n + 1;
}

pub fn main() void {
    frPrint("start\n", .{});
    frPrint("apply {d}\n", .{applyTo(addOne, 6)});
    frPrint("twice {d}\n", .{twice(addOne, 10)});
    frPrint("done\n", .{});
}
