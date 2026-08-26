const std = @import("std");

fn frPrint(comptime format: []const u8, args: anytype) void {
    var buffer: [1024]u8 = undefined;
    var writer = std.Io.File.stdout().writerStreaming(std.Options.debug_io, &buffer);
    writer.interface.print(format, args) catch unreachable;
    writer.interface.flush() catch unreachable;
}

pub fn main() void {
    frPrint("start\n", .{});
    var seen = std.StringHashMap(bool).init(std.heap.page_allocator);
    seen.put("ada", true) catch unreachable;
    seen.put("alan", true) catch unreachable;
    seen.put("ada", true) catch unreachable;
    frPrint("size {d}\n", .{seen.count()});
    if (seen.contains("ada")) {
        frPrint("has-ada yes\n", .{});
    } else {
        frPrint("has-ada no\n", .{});
    }
    if (seen.contains("grace")) {
        frPrint("has-grace yes\n", .{});
    } else {
        frPrint("has-grace no\n", .{});
    }
    _ = seen.remove("alan");
    frPrint("after {d}\n", .{seen.count()});
    frPrint("done\n", .{});
}
