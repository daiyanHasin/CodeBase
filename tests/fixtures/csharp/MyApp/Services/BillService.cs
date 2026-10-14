using MyApp;

namespace MyApp.Services;

public class BillService
{
    public decimal Total(Bill bill)
    {
        return bill.Amount;
    }

    private static string Format(decimal value) => value.ToString("0.00");
}
