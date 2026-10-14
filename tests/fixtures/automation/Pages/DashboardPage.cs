public class DashboardPage : BasePage { public bool IsShown() { return Driver.FindElement(By.Id("dash")).Displayed; } }
