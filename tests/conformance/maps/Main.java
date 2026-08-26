import java.util.HashMap;
import java.util.Map;

public class Main {
    public static void main(String[] args) {
        System.out.println("start");
        Map<String, Long> ages = new HashMap<>();
        ages.put("ada", 36L);
        ages.put("alan", 41L);
        ages.put("grace", 45L);
        System.out.println("size " + ages.size());
        System.out.println("ada " + ages.get("ada"));
        long total = 0;
        for (String name : new String[] {"ada", "alan", "grace"}) {
            total = total + ages.get(name);
        }
        System.out.println("total " + total);
        System.out.println("done");
    }
}
