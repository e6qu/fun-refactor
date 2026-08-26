import java.util.ArrayList;
import java.util.List;

public class Main {
    public static void main(String[] args) {
        System.out.println("start");
        List<Integer> nums = new ArrayList<>(List.of(1, 2, 3, 4));
        List<Integer> doubled = new ArrayList<>();
        for (Integer n : nums) {
            doubled.add(n * 2);
        }
        System.out.println("first " + doubled.get(0));
        int total = 0;
        for (Integer d : doubled) {
            total = total + d;
        }
        System.out.println("total " + total);
        List<Integer> big = new ArrayList<>();
        for (Integer n : nums) {
            if (n > 2) {
                big.add(n);
            }
        }
        int kept = 0;
        for (Integer b : big) {
            kept = kept + b;
        }
        System.out.println("kept " + kept);
        System.out.println("done");
    }
}
