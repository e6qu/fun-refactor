import java.util.List;

public final class Main {
    static final class Box {
        int value;

        Box(int value) {
            this.value = value;
        }

        int get() {
            return this.value;
        }
    }

    static int firstOf(List<Integer> items) {
        return items.get(0);
    }

    static int countOf(List<String> items) {
        return items.size();
    }

    public static void main(String[] args) {
        System.out.println("start");
        List<Integer> numbers = List.of(4, 5, 6);
        List<String> words = List.of("a", "b");
        System.out.println("first " + firstOf(numbers));
        System.out.println("count " + countOf(words));
        Box b = new Box(9);
        System.out.println("box " + b.get());
        System.out.println("done");
    }
}
