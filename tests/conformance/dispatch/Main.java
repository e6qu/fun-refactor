public class Main {
    static String dayName(int day) {
        switch (day) {
            case 1:
                return "mon";
            case 2:
                return "tue";
            case 3:
                return "wed";
            default:
                return "other";
        }
    }

    static String opKind(String word) {
        switch (word) {
            case "add":
                return "plus";
            case "sub":
                return "minus";
            default:
                return "other";
        }
    }

    public static void main(String[] args) {
        System.out.println("day 1 " + dayName(1));
        System.out.println("day 3 " + dayName(3));
        System.out.println("day 9 " + dayName(9));
        System.out.println("kind add " + opKind("add"));
        System.out.println("kind sub " + opKind("sub"));
        System.out.println("kind mul " + opKind("mul"));
        System.out.println("done");
    }
}
