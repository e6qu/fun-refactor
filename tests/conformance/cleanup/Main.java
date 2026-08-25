public class Main {
    static void work() {
        System.out.println("open a");
        try {
            System.out.println("open b");
            try {
                System.out.println("work");
            } finally {
                System.out.println("close b");
            }
        } finally {
            System.out.println("close a");
        }
    }

    public static void main(String[] args) {
        work();
        System.out.println("done");
    }
}
