const assert = require('node:assert/strict');
const { receipt } = require('./target/js/checkout.js');
const { localReceipt } = require('./target/js/unrelated.js');

for (const amount of [-5, 0, 5]) {
    assert.equal(receipt(amount), amount + 2);
    assert.equal(localReceipt(amount), amount + 100);
}
console.log('public receipt checks passed');
