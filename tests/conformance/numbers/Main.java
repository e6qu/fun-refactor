public final class Main {
    static int floorDiv(int a, int b) {
        int quotient = a / b;
        if (a % b != 0 && (a < 0) != (b < 0)) {
            return quotient - 1;
        }
        return quotient;
    }

    static int floorMod(int a, int b) {
        return a - floorDiv(a, b) * b;
    }

    public static void main(String[] args) {
        System.out.println("start");
        int a = 7;
        int b = 2;
        System.out.println("sum " + (a + b));
        System.out.println("diff " + (a - b));
        System.out.println("product " + (a * b));
        System.out.println("quotient " + floorDiv(a, b));
        System.out.println("remainder " + floorMod(a, b));
        int negative = -7;
        System.out.println("negquotient " + floorDiv(negative, b));
        System.out.println("negremainder " + floorMod(negative, b));
        System.out.println("done");
    }
}
