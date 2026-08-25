public class Main {
    static int load(String name, int base) {
        System.out.println("fetch " + name);
        return base + 1;
    }

    static int total(int a, int b) {
        int first = load("a", a);
        int second = load("b", b);
        return first + second;
    }

    public static void main(String[] args) {
        System.out.println("start");
        int result = total(10, 20);
        System.out.println("total " + result);
        System.out.println("done");
    }
}
