import java.util.ArrayList;

public class Main {
    public static void main(String[] args) {
        ArrayList<Integer> nums = new ArrayList<>();
        nums.add(3);
        nums.add(1);
        nums.add(2);
        System.out.println("len " + nums.size());
        System.out.println("first " + nums.get(0));
        nums.set(1, 10);
        int total = 0;
        for (int value : nums) {
            total = total + value;
        }
        System.out.println("sum " + total);
        for (int value : nums) {
            System.out.println("item " + value);
        }
        System.out.println("done");
    }
}
