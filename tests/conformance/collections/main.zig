const std = @import("std");

fn frPrint(comptime format: []const u8, args: anytype) void {
    var buffer: [4096]u8 = undefined;
    var writer = std.Io.File.stdout().writerStreaming(std.Options.debug_io, &buffer);
    writer.interface.print(format, args) catch unreachable;
    writer.interface.flush() catch unreachable;
}

pub fn main() void {
    var nums: std.ArrayList(i64) = .empty;
    nums.append(std.heap.page_allocator, 3) catch unreachable;
    nums.append(std.heap.page_allocator, 1) catch unreachable;
    nums.append(std.heap.page_allocator, 2) catch unreachable;
    frPrint("len {d}\n", .{nums.items.len});
    frPrint("first {d}\n", .{nums.items[0]});
    nums.items[1] = 10;
    var total: i64 = 0;
    for (nums.items) |value| {
        total = total + value;
    }
    frPrint("sum {d}\n", .{total});
    for (nums.items) |value| {
        frPrint("item {d}\n", .{value});
    }
    frPrint("done\n", .{});
}
