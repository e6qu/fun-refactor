const std = @import("std");

fn frPrint(comptime format: []const u8, args: anytype) void {
    var buffer: [4096]u8 = undefined;
    var writer = std.Io.File.stdout().writerStreaming(std.Options.debug_io, &buffer);
    writer.interface.print(format, args) catch unreachable;
    writer.interface.flush() catch unreachable;
}

fn load(name: []const u8, base: i64) i64 {
    frPrint("fetch {s}\n", .{name});
    return base + 1;
}

fn total(a: i64, b: i64) i64 {
    const first = load("a", a);
    const second = load("b", b);
    return first + second;
}

pub fn main() void {
    frPrint("start\n", .{});
    const result = total(10, 20);
    frPrint("total {d}\n", .{result});
    frPrint("done\n", .{});
}
