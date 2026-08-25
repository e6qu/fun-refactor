public class Main {
    static int check(int n) {
        if (n < 0) {
            throw new RuntimeException("negative");
        }
        return n * 2;
    }

    static int twice(int n) {
        return check(n) + 1;
    }

    public static void main(String[] args) {
        try {
            int v = check(5);
            System.out.println("checked 5 -> " + v);
        } catch (RuntimeException e) {
            System.out.println("caught " + e.getMessage());
        }
        try {
            int v = check(-3);
            System.out.println("never " + v);
        } catch (RuntimeException e) {
            System.out.println("caught " + e.getMessage());
        }
        try {
            int v = twice(4);
            System.out.println("double 4 -> " + v);
        } catch (RuntimeException e) {
            System.out.println("caught " + e.getMessage());
        }
        try {
            int v = twice(-2);
            System.out.println("never " + v);
        } catch (RuntimeException e) {
            System.out.println("caught " + e.getMessage());
        }
        System.out.println("done");
    }
}
