public class Main {
    public static void main(String[] args) {
        System.out.println("start");
        int n = 42;
        int total = n + 10;
        System.out.println("n " + n);
        System.out.println("sum " + total);
        total = total * 2;
        System.out.println("twice " + total);
        int q = 10 / 3;
        int r = 10 % 3;
        System.out.println("q " + q + " r " + r);
        String label = "item-" + 7;
        System.out.println("label " + label);
        int i = 0;
        while (i < 3) {
            System.out.println("tick " + i);
            i = i + 1;
        }
        System.out.println("done");
    }
}
