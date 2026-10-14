public class LoginPage : BasePage {
    By user = By.Id("user"); By pass = By.Id("pass");
    public void Open() { Driver.Navigate().GoToUrl(Settings.BaseUrl); }
    public void LogIn(string u, string p) { Driver.FindElement(user).SendKeys(u); }
    public string Error() { return Driver.FindElement(By.Id("err")).Text; }
}
