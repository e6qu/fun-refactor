import java.util.function.Function;

public final class Main {
    static int applyTo(Function<Integer, Integer> f, int n) {
        return f.apply(n);
    }

    static int twice(Function<Integer, Integer> f, int n) {
        return f.apply(f.apply(n));
    }

    public static void main(String[] args) {
        System.out.println("start");
        Function<Integer, Integer> addOne = (n) -> n + 1;
        System.out.println("apply " + applyTo(addOne, 6));
        System.out.println("twice " + twice(addOne, 10));
        System.out.println("done");
    }
}
