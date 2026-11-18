function adjust(a, b) {
    return a + b;
}

const h = {
    get value() {
        n++;
        return 1;
    }
};
let n = 2;
adjust(h.value, n);
