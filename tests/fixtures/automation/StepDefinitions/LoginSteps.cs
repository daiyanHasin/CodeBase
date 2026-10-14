using Reqnroll;
[Binding]
public class LoginSteps {
    private readonly LoginPage _login;
    private readonly DashboardPage _dash;
    [Given(@"the browser is open")] public void Open() { _login.Open(); }
    [When(@"I log in as ""(.*)""")] public void LogIn(string user) { var u = TestDataReader.User(user); _login.LogIn(u.Name, u.Password); }
    [Then(@"I see the dashboard")] public void SeeDashboard() { Assert.That(_dash.IsShown()); }
    [Then(@"I see the error ""(.*)""")] public void SeeError(string msg) { Assert.That(_login.Error() == msg); }
    [Then(@"I log out")] public void LogOut() { }
}
