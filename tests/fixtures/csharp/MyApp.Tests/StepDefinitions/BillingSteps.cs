using Reqnroll;
using MyApp.Services;

[Binding]
public class BillingSteps
{
    private readonly BillService _service = new();

    [Given(@"a bill of (\d+) taka")]
    public void GivenABill(int amount) { }

    [When("I ask for the total")]
    public void WhenIAsk() { }

    [Then("the total is {int}")]
    public void ThenTotal(int expected) { }
}
