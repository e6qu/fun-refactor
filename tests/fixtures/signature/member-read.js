function adjust(a, b) {
    return a * 10 + b;
}

class Holder {
    get value() {
        console.log("read");
        return 2;
    }

    run(n) {
        return adjust(this.value, n);
    }
}

const holder = new Holder();
console.log(holder.run(3));
console.log(adjust(holder.value, 4));
