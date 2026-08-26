const std = @import("std");

fn frPrint(comptime format: []const u8, args: anytype) void {
    var buffer: [1024]u8 = undefined;
    var writer = std.Io.File.stdout().writerStreaming(std.Options.debug_io, &buffer);
    writer.interface.print(format, args) catch unreachable;
    writer.interface.flush() catch unreachable;
}

pub fn main() void {
    frPrint("start\n", .{});
    var ages = std.StringHashMap(i64).init(std.heap.page_allocator);
    ages.put("ada", 36) catch unreachable;
    ages.put("alan", 41) catch unreachable;
    ages.put("grace", 45) catch unreachable;
    frPrint("size {d}\n", .{ages.count()});
    frPrint("ada {d}\n", .{ages.get("ada").?});
    var total: i64 = 0;
    for ([_][]const u8{ "ada", "alan", "grace" }) |name| {
        total = total + ages.get(name).?;
    }
    frPrint("total {d}\n", .{total});
    frPrint("done\n", .{});
}
