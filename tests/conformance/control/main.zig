const std = @import("std");

fn classify(n: i64) []const u8 {
    if (n < 0) {
        return "negative";
    } else if (n == 0) {
        return "zero";
    } else if (n < 10) {
        return "small";
    }
    return "large";
}

pub fn main(init: std.process.Init) !void {
    var buffer: [4096]u8 = undefined;
    var writer = std.Io.File.stdout().writer(init.io, &buffer);
    const out = &writer.interface;
    try out.print("classify -5 {s}\n", .{classify(-5)});
    try out.print("classify 0 {s}\n", .{classify(0)});
    try out.print("classify 7 {s}\n", .{classify(7)});
    try out.print("classify 40 {s}\n", .{classify(40)});
    var i: i64 = 0;
    while (i < 6) {
        i = i + 1;
        if (@mod(i, 2) == 0) {
            continue;
        }
        if (i == 5) {
            break;
        }
        try out.print("odd {d}\n", .{i});
    }
    for ([_]i64{ 3, 1, 2 }) |value| {
        try out.print("item {d}\n", .{value});
    }
    var outer: i64 = 0;
    while (outer < 3) {
        var inner: i64 = 0;
        while (inner < 3) {
            if (inner == 2) {
                break;
            }
            try out.print("pair {d} {d}\n", .{ outer, inner });
            inner = inner + 1;
        }
        outer = outer + 1;
    }
    try out.print("done\n", .{});
    try out.flush();
}
