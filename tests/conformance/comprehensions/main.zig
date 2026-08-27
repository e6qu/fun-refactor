const std = @import("std");

fn frPrint(comptime format: []const u8, args: anytype) void {
    var buffer: [1024]u8 = undefined;
    var writer = std.Io.File.stdout().writerStreaming(std.Options.debug_io, &buffer);
    writer.interface.print(format, args) catch unreachable;
    writer.interface.flush() catch unreachable;
}

pub fn main() void {
    frPrint("start\n", .{});
    const nums = [_]i64{ 1, 2, 3, 4 };
    var doubled: std.ArrayList(i64) = .empty;
    for (nums) |n| {
        doubled.append(std.heap.page_allocator, n * 2) catch unreachable;
    }
    frPrint("first {d}\n", .{doubled.items[0]});
    var total: i64 = 0;
    for (doubled.items) |d| {
        total = total + d;
    }
    frPrint("total {d}\n", .{total});
    var big: std.ArrayList(i64) = .empty;
    for (nums) |n| {
        if (n > 2) {
            big.append(std.heap.page_allocator, n) catch unreachable;
        }
    }
    var kept: i64 = 0;
    for (big.items) |b| {
        kept = kept + b;
    }
    frPrint("kept {d}\n", .{kept});
    frPrint("done\n", .{});
}
