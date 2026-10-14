public static class TestDataReader { public static User User(string key) { var json = File.ReadAllText("TestData/users.json"); return null; } }
public class User { public string Name; public string Password; }
