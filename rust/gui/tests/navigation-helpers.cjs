const openMenu = async page => { if (await page.locator('#navigation-toggle').isVisible() && !await page.locator('#sidebar').isVisible()) await page.locator('#navigation-toggle').click(); };
const navigate = async (page,view) => { await openMenu(page); await page.locator(`#nav-${view}`).click(); };
module.exports={openMenu,navigate};
