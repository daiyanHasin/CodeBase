public static class DriverFactory { public static IWebDriver Current; public static void Start() { Current = new ChromeDriver(); } public static void Stop() { Current.Quit(); } }
