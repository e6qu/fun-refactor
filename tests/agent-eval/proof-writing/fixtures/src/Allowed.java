public class Allowed {
    public static boolean allowed(boolean staff, boolean owner) {
        if (staff) {
            return true;
        }
        return owner;
    }
}
