const std = @import("std");

fn check(n: i64) error{negative}!i64 {
    if (n < 0) {
        return error.negative;
    }
    return n * 2;
}

fn double(n: i64) error{negative}!i64 {
    return try check(n) + 1;
}

pub fn main(init: std.process.Init) !void {
    var buffer: [4096]u8 = undefined;
    var writer = std.Io.File.stdout().writer(init.io, &buffer);
    const out = &writer.interface;
    if (check(5)) |v| {
        try out.print("checked 5 -> {d}\n", .{v});
    } else |e| {
        try out.print("caught {s}\n", .{@errorName(e)});
    }
    if (check(-3)) |v| {
        try out.print("never {d}\n", .{v});
    } else |e| {
        try out.print("caught {s}\n", .{@errorName(e)});
    }
    if (double(4)) |v| {
        try out.print("double 4 -> {d}\n", .{v});
    } else |e| {
        try out.print("caught {s}\n", .{@errorName(e)});
    }
    if (double(-2)) |v| {
        try out.print("never {d}\n", .{v});
    } else |e| {
        try out.print("caught {s}\n", .{@errorName(e)});
    }
    try out.print("done\n", .{});
    try out.flush();
}
