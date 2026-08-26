import java.util.HashSet;
import java.util.Set;

public final class Main {
    public static void main(String[] args) {
        System.out.println("start");
        Set<String> seen = new HashSet<>();
        seen.add("ada");
        seen.add("alan");
        seen.add("ada");
        System.out.println("size " + seen.size());
        if (seen.contains("ada")) {
            System.out.println("has-ada yes");
        } else {
            System.out.println("has-ada no");
        }
        if (seen.contains("grace")) {
            System.out.println("has-grace yes");
        } else {
            System.out.println("has-grace no");
        }
        seen.remove("alan");
        System.out.println("after " + seen.size());
        System.out.println("done");
    }
}
