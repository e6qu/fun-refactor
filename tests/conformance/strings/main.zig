const std = @import("std");

fn frPrint(comptime format: []const u8, args: anytype) void {
    var buffer: [4096]u8 = undefined;
    var writer = std.Io.File.stdout().writerStreaming(std.Options.debug_io, &buffer);
    writer.interface.print(format, args) catch unreachable;
    writer.interface.flush() catch unreachable;
}

fn frFormat(comptime format: []const u8, args: anytype) []u8 {
    return std.fmt.allocPrint(std.heap.page_allocator, format, args) catch unreachable;
}

pub fn main() void {
    const word = "Hello";
    const upper = std.ascii.allocUpperString(std.heap.page_allocator, word) catch unreachable;
    frPrint("upper {s}\n", .{upper});
    const lower = std.ascii.allocLowerString(std.heap.page_allocator, word) catch unreachable;
    frPrint("lower {s}\n", .{lower});
    frPrint("len {d}\n", .{word.len});
    const joined = frFormat("{s}-{s}", .{ word, "World" });
    frPrint("concat {s}\n", .{joined});
    if (std.mem.indexOf(u8, word, "ell") != null) {
        frPrint("has yes\n", .{});
    }
    if (std.mem.indexOf(u8, word, "xyz") != null) {
        frPrint("never\n", .{});
    } else {
        frPrint("has no\n", .{});
    }
    frPrint("done\n", .{});
}
