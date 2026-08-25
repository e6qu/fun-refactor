const std = @import("std");

fn frPrint(comptime format: []const u8, args: anytype) void {
    var buffer: [4096]u8 = undefined;
    var writer = std.Io.File.stdout().writerStreaming(std.Options.debug_io, &buffer);
    writer.interface.print(format, args) catch unreachable;
    writer.interface.flush() catch unreachable;
}

fn dayName(day: i64) []const u8 {
    switch (day) {
        1 => {
            return "mon";
        },
        2 => {
            return "tue";
        },
        3 => {
            return "wed";
        },
        else => {
            return "other";
        },
    }
}

fn opKind(word: []const u8) []const u8 {
    if (std.mem.eql(u8, word, "add")) {
        return "plus";
    } else if (std.mem.eql(u8, word, "sub")) {
        return "minus";
    }
    return "other";
}

pub fn main() void {
    frPrint("day 1 {s}\n", .{dayName(1)});
    frPrint("day 3 {s}\n", .{dayName(3)});
    frPrint("day 9 {s}\n", .{dayName(9)});
    frPrint("kind add {s}\n", .{opKind("add")});
    frPrint("kind sub {s}\n", .{opKind("sub")});
    frPrint("kind mul {s}\n", .{opKind("mul")});
    frPrint("done\n", .{});
}
