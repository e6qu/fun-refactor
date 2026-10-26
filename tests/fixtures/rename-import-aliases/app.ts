import { calculate as display } from './numbers';
import { calculate as alternate } from './other';
function calculate(n) { return n - 1; }
function run(n) { return display(n) + alternate(n) + calculate(n); }
function shadow(display) { return display(1); }
