public class Main {
    public static void main(String[] args) {
        String word = "Hello";
        System.out.println("upper " + word.toUpperCase());
        System.out.println("lower " + word.toLowerCase());
        System.out.println("len " + word.length());
        String joined = word + "-" + "World";
        System.out.println("concat " + joined);
        if (word.contains("ell")) {
            System.out.println("has yes");
        }
        if (word.contains("xyz")) {
            System.out.println("never");
        } else {
            System.out.println("has no");
        }
        System.out.println("done");
    }
}
