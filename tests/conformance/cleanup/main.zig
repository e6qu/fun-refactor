const std = @import("std");

fn frPrint(comptime format: []const u8, args: anytype) void {
    var buffer: [4096]u8 = undefined;
    var writer = std.Io.File.stdout().writerStreaming(std.Options.debug_io, &buffer);
    writer.interface.print(format, args) catch unreachable;
    writer.interface.flush() catch unreachable;
}

fn work() void {
    frPrint("open a\n", .{});
    defer frPrint("close a\n", .{});
    frPrint("open b\n", .{});
    defer frPrint("close b\n", .{});
    frPrint("work\n", .{});
}

pub fn main() void {
    work();
    frPrint("done\n", .{});
}
