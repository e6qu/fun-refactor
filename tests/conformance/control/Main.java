public class Main {
    static String classify(int n) {
        if (n < 0) {
            return "negative";
        } else if (n == 0) {
            return "zero";
        } else if (n < 10) {
            return "small";
        }
        return "large";
    }

    public static void main(String[] args) {
        System.out.println("classify -5 " + classify(-5));
        System.out.println("classify 0 " + classify(0));
        System.out.println("classify 7 " + classify(7));
        System.out.println("classify 40 " + classify(40));
        int i = 0;
        while (i < 6) {
            i = i + 1;
            if (i % 2 == 0) {
                continue;
            }
            if (i == 5) {
                break;
            }
            System.out.println("odd " + i);
        }
        for (int value : new int[] { 3, 1, 2 }) {
            System.out.println("item " + value);
        }
        int outer = 0;
        while (outer < 3) {
            int inner = 0;
            while (inner < 3) {
                if (inner == 2) {
                    break;
                }
                System.out.println("pair " + outer + " " + inner);
                inner = inner + 1;
            }
            outer = outer + 1;
        }
        System.out.println("done");
    }
}
