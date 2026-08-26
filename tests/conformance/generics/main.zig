const std = @import("std");

fn frPrint(comptime format: []const u8, args: anytype) void {
    var buffer: [1024]u8 = undefined;
    var writer = std.Io.File.stdout().writerStreaming(std.Options.debug_io, &buffer);
    writer.interface.print(format, args) catch unreachable;
    writer.interface.flush() catch unreachable;
}

const Box = struct {
    value: i64,

    fn get(self: Box) i64 {
        return self.value;
    }
};

fn firstOf(items: []const i64) i64 {
    return items[0];
}

fn countOf(items: []const []const u8) i64 {
    return @intCast(items.len);
}

pub fn main() void {
    frPrint("start\n", .{});
    const numbers = [_]i64{ 4, 5, 6 };
    const words = [_][]const u8{ "a", "b" };
    frPrint("first {d}\n", .{firstOf(&numbers)});
    frPrint("count {d}\n", .{countOf(&words)});
    const b = Box{ .value = 9 };
    frPrint("box {d}\n", .{b.get()});
    frPrint("done\n", .{});
}
