using MyApp.Services;
using Xunit;

public class BillServiceTests
{
    [Fact]
    public void Total_returns_amount()
    {
        var s = new BillService();
        Assert.Equal(5m, s.Total(new MyApp.Bill { Amount = 5m }));
    }
}
