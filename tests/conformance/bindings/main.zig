const std = @import("std");

pub fn main(init: std.process.Init) !void {
    var buffer: [4096]u8 = undefined;
    var writer = std.Io.File.stdout().writer(init.io, &buffer);
    const out = &writer.interface;
    try out.print("start\n", .{});
    const n: i64 = 42;
    var total: i64 = n + 10;
    try out.print("n {d}\n", .{n});
    try out.print("sum {d}\n", .{total});
    total = total * 2;
    try out.print("twice {d}\n", .{total});
    const q: i64 = @divTrunc(10, 3);
    const r: i64 = @mod(10, 3);
    try out.print("q {d} r {d}\n", .{ q, r });
    try out.print("label item-{d}\n", .{7});
    var i: i64 = 0;
    while (i < 3) {
        try out.print("tick {d}\n", .{i});
        i = i + 1;
    }
    try out.print("done\n", .{});
    try out.flush();
}
